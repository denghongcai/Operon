#!/usr/bin/env python3
"""Real daemon/CLI across disjoint owned Docker networks; no TCP bridge."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import uuid
import urllib.request


def main():
    timeout = int(os.environ.get("OPERON_EMBEDDED_TEST_TIMEOUT_SECS", "180"))
    if timeout <= 0:
        raise ValueError("acceptance timeout must be positive")
    image = os.environ.get("OPERON_NATIVE_TEST_IMAGE", "operon-native-iroh-acceptance")
    relay_image = os.environ.get("OPERON_RELAY_TEST_IMAGE", "operon-iroh-relay-acceptance")
    prefix = "operon-relay-" + uuid.uuid4().hex[:12]
    networks, containers = [], []

    def run(*args, check=True):
        result = subprocess.run(args, text=True, capture_output=True, timeout=timeout)
        if check and result.returncode:
            raise RuntimeError(f"acceptance command failed ({result.returncode}): {args}\n{result.stderr}")
        return result

    def docker(*args):
        return run("docker", *args).stdout.strip()

    def start(name, *args):
        cid = docker("run", "-d", "--name", name, "--label", "operon.acceptance=embedded-relay", *args)
        if not re.fullmatch(r"[a-f0-9]{64}", cid):
            raise AssertionError("invalid owned container ID")
        containers.append(cid)
        return cid

    def wait_public_id(server):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            logs = re.sub(r"\x1b\[[0-9;]*m", "", docker("logs", server))
            match = re.search(r"endpoint_id=([a-f0-9]{64})", logs)
            if match:
                return match.group(1)
            if docker("inspect", "--format", "{{.State.Running}}", server) != "true":
                raise AssertionError("native daemon exited: " + logs)
            time.sleep(0.2)
        raise AssertionError("native daemon readiness timed out: " + docker("logs", server))

    try:
        with tempfile.TemporaryDirectory(prefix=prefix) as temp:
            fixtures = Path(temp)
            for suffix in ("server", "client"):
                networks.append(docker("network", "create", "--internal", prefix + "-" + suffix))
            relay = start(prefix + "-relay", "--network", networks[0], "--network-alias", "relay",
                          relay_image)
            docker("network", "connect", "--alias", "relay", networks[1], relay)
            endpoint_config = {"identity_file": "/state/server.key", "bind_addrs": [],
                               "relay_urls": ["http://relay:3340"], "allow_insecure_relays": True,
                               "readiness_timeout_secs": timeout}
            server_config = {"version": 1, "daemon": {"node_id": "relay-server", "grpc_listen": "127.0.0.1:7789",
                             "workspace": "/workspace", "auth": {"token": "isolated-test-token"}, "iroh": endpoint_config},
                             "policy": {"subject": "relay-acceptance", "fs": {"mounts": [{"name": "workspace", "path": "/",
                             "permissions": {"read": True, "write": True, "delete": True}}]}, "exec": {
                             "allowed_cwds": ["/"], "default_timeout_secs": 30, "max_timeout_secs": 300,
                             "preserve_env": False, "env_allowlist": [], "allowed_secrets": []}}}
            server_path = fixtures / "server.yaml"
            server_path.write_text(json.dumps(server_config))
            server_path.chmod(0o600)
            server = start(prefix + "-server", "--network", networks[0], "--user", "0:0", "--tmpfs", "/state",
                           "--mount", f"type=bind,src={server_path},dst=/config.yaml,readonly",
                           "--entrypoint", "operond", image, "start", "--config", "/config.yaml")
            identity = wait_public_id(server)
            client_local = dict(endpoint_config, identity_file="/state/client.key", peer_relay_urls=["http://relay:3340"])
            client_config = {"version": 1, "client": {"nodes": {"remote": {"endpoint": "iroh://" + identity,
                             "auth": {"token": "isolated-test-token"}, "transport": {"connect_timeout_secs": timeout,
                             "rpc_timeout_secs": timeout, "iroh": client_local}}}}}
            client_path = fixtures / "client.yaml"
            client_path.write_text(json.dumps(client_config))
            client_path.chmod(0o600)
            payload = bytes(range(251)) * (8 * 1024 * 1024 // 251 + 1)
            (fixtures / "payload.bin").write_bytes(payload)
            client = start(prefix + "-client", "--network", networks[1], "--user", "0:0", "--tmpfs", "/state",
                           "--mount", f"type=bind,src={fixtures},dst=/fixtures,readonly",
                           "--entrypoint", "sleep", image, "infinity")

            def cli(*args):
                return docker("exec", client, "operon", "--config", "/fixtures/client.yaml", *args)

            # Topology is the proof: no common network, no IP transports, and TCP
            # daemon listener is loopback-only inside the server container.
            cli("node", "ping", "remote")
            denied_config = json.loads(json.dumps(client_config))
            denied_config["client"]["nodes"]["remote"]["auth"]["token"] = "wrong-test-token"
            denied_path = fixtures / "denied.yaml"
            denied_path.write_text(json.dumps(denied_config))
            denied_path.chmod(0o600)
            denied = run("docker", "exec", client, "operon", "--config", "/fixtures/denied.yaml",
                         "node", "ping", "remote", check=False)
            assert denied.returncode != 0 and "invalid bearer token" in denied.stderr.lower(), f"invalid bearer rejection diagnostic: exit={denied.returncode}, stderr={denied.stderr}"
            execution = cli("exec", "run", "remote", "--cwd", "/", "printf relay-exec-verified")
            assert "relay-exec-verified" in execution, "native relayed exec output missing"
            cli("fs", "write", "remote:/relay.bin", "--file", "/fixtures/payload.bin")
            cli("fs", "read", "remote:/relay.bin", "--output", "/state/result.bin")
            actual = docker("exec", client, "sha256sum", "/state/result.bin").split()[0]
            assert actual == hashlib.sha256(payload).hexdigest(), "relayed file corrupted"
            addresses = json.loads(docker("inspect", "--format", "{{json .NetworkSettings.Networks}}", relay))
            metrics_host = next(iter(addresses.values()))["IPAddress"]
            # Internal networks have no published ports. Query only the owned
            # local bridge address, without sending requests through a proxy.
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            with opener.open("http://" + metrics_host + ":9090/metrics", timeout=timeout) as response:
                metrics = response.read().decode()
            for counter in ("relayserver_bytes_sent_total", "relayserver_bytes_recv_total"):
                value = float(re.search(r"^" + counter + r" ([0-9.e+]+)$", metrics, re.M).group(1))
                assert value > 2 * len(payload), "insufficient real daemon relay traffic"
                print(counter + "=" + str(int(value)))
            docker("restart", "--time", "5", relay)
            cli("node", "ping", "remote")
            cli("fs", "read", "remote:/relay.bin", "--output", "/state/after-restart.bin")
            recovered = docker("exec", client, "sha256sum", "/state/after-restart.bin").split()[0]
            assert recovered == actual, "post-relay-restart data corrupted"
            print("PASS real native daemon/CLI relay-only auth, exec, >8MiB file roundtrip and relay restart recovery across disjoint networks")
    finally:
        for cid in reversed(containers):
            result = run("docker", "rm", "-f", cid, check=False)
            if result.returncode:
                print("owned container cleanup failed: " + cid)
        for network in reversed(networks):
            run("docker", "network", "rm", network, check=False)


if __name__ == "__main__":
    main()
