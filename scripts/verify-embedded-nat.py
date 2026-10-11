#!/usr/bin/env python3
"""Two owned SNAT routers, private endpoint LANs and self-hosted TLS relay."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import urllib.request
import uuid


def main():
    timeout = int(os.environ.get("OPERON_EMBEDDED_TEST_TIMEOUT_SECS", "180"))
    if timeout <= 0:
        raise ValueError("test timeout must be positive")
    native = os.environ.get("OPERON_NAT_NATIVE_IMAGE", "operon-iroh-nat-native")
    relay_image = os.environ.get("OPERON_RELAY_TEST_IMAGE", "operon-iroh-relay-acceptance")
    router_image = os.environ.get("OPERON_UDP_FILTER_IMAGE", "operon-iroh-udp-filter")
    prefix = "operon-nat-" + uuid.uuid4().hex[:12]
    containers, networks = [], []

    def run(*args, check=True):
        result = subprocess.run(args, text=True, capture_output=True, timeout=timeout)
        if check and result.returncode:
            raise RuntimeError(f"owned NAT command failed: {args}\n{result.stderr}")
        return result

    def docker(*args):
        return run("docker", *args).stdout.strip()

    def start(name, *args):
        cid = docker("run", "-d", "--name", prefix + "-" + name,
                     "--label", "operon.acceptance=embedded-nat", *args)
        if not re.fullmatch(r"[a-f0-9]{64}", cid):
            raise AssertionError("invalid owned container ID")
        containers.append(cid)
        return cid

    def addresses(cid):
        return json.loads(docker("inspect", "--format", "{{json .NetworkSettings.Networks}}", cid))

    def namespace(cid, command):
        return docker("run", "--rm", "--network", "container:" + cid,
                      "--cap-drop", "ALL", "--cap-add", "NET_ADMIN", router_image, command)

    def wait(predicate, detail):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(0.2)
        raise AssertionError("owned NAT readiness failed: " + detail())

    try:
        with tempfile.TemporaryDirectory(prefix=prefix) as temp:
            fixtures = Path(temp)
            fixtures.chmod(0o755)  # Relay's non-root UID reads only test TLS fixtures.
            for role in ("server", "client", "transit"):
                # Docker --internal LAN bridges discard off-subnet packets
                # before the owned router can receive them. Endpoint default
                # routes are replaced below; owned router rules allow only the
                # isolated transit subnet, never Internet egress.
                options = ["--internal"] if role == "transit" else []
                networks.append(docker("network", "create", *options, prefix + "-" + role))
            relay = start("relay", "--network", networks[2], "--mount",
                          f"type=bind,src={fixtures},dst=/fixtures,readonly", "--entrypoint", "sleep",
                          relay_image, "infinity")
            relay_ip = addresses(relay)[prefix + "-transit"]["IPAddress"]
            run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                "-subj", "/CN=owned-operon-relay-CA", "-addext", "basicConstraints=critical,CA:TRUE",
                "-addext", "keyUsage=critical,keyCertSign,cRLSign",
                "-keyout", str(fixtures / "ca.key"), "-out", str(fixtures / "ca.pem"))
            run("openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=owned-operon-relay",
                "-keyout", str(fixtures / "relay.key"), "-out", str(fixtures / "relay.csr"))
            (fixtures / "leaf.ext").write_text(f"subjectAltName=IP:{relay_ip}\nbasicConstraints=critical,CA:FALSE\n"
                "keyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n")
            run("openssl", "x509", "-req", "-in", str(fixtures / "relay.csr"), "-CA", str(fixtures / "ca.pem"),
                "-CAkey", str(fixtures / "ca.key"), "-CAcreateserial", "-days", "1", "-extfile", str(fixtures / "leaf.ext"),
                "-out", str(fixtures / "relay.pem"))
            (fixtures / "relay.key").chmod(0o644)  # Ephemeral test-only certificate, destroyed on exit.
            (fixtures / "relay.toml").write_text('enable_relay = true\nhttp_bind_addr = "0.0.0.0:3340"\n'
                'enable_quic_addr_discovery = true\nenable_metrics = true\nmetrics_bind_addr = "0.0.0.0:9090"\n'
                '[tls]\nhttps_bind_addr = "0.0.0.0:443"\nquic_bind_addr = "0.0.0.0:7842"\n'
                'cert_mode = "Manual"\nmanual_cert_path = "/fixtures/relay.pem"\nmanual_key_path = "/fixtures/relay.key"\n')
            docker("exec", "-d", relay, "sh", "-c", "iroh-relay --config-path /fixtures/relay.toml >/tmp/relay.log 2>&1")
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

            def metrics_ready():
                try:
                    with opener.open(f"http://{relay_ip}:9090/metrics", timeout=2) as response:
                        return response.status == 200
                except OSError:
                    return False
            wait(metrics_ready, lambda: docker("exec", relay, "cat", "/tmp/relay.log"))
            routers = []
            for index, role in enumerate(("server", "client")):
                router = start(role + "-router", "--network", networks[index], "--cap-drop", "ALL",
                    "--cap-add", "NET_ADMIN", "--sysctl", "net.ipv4.ip_forward=1",
                    "--entrypoint", "sleep", router_image, "infinity")
                docker("network", "connect", networks[2], router)
                routers.append(router)
            router_public = [addresses(router)[prefix + "-transit"]["IPAddress"] for router in routers]
            if os.environ.get("OPERON_NAT_CAPTURE", "0") == "1":
                for index, router in enumerate(routers):
                    start("capture-" + str(index), "--network", "container:" + router, "--cap-drop", "ALL",
                          "--cap-add", "NET_RAW", "--entrypoint", "tcpdump", router_image,
                          "-p", "-l", "-n", "-i", "any", "udp", "and", "not", "port", "7842")
            transit_subnet = json.loads(docker("network", "inspect", networks[2]))[0]["IPAM"]["Config"][0]["Subnet"]
            for index, router in enumerate(routers):
                subnet = json.loads(docker("network", "inspect", networks[index]))[0]["IPAM"]["Config"][0]["Subnet"]
                # These namespaces are routers, with no UDP services of their
                # own. Drop unsolicited router-local input before conntrack
                # confirmation: accepting it reserves the reverse tuple and
                # makes MASQUERADE change the next endpoint's source port.
                namespace(router, f"iptables -t nat -A POSTROUTING -s {subnet} -d {transit_subnet} -j MASQUERADE; "
                    f"iptables -t raw -A PREROUTING -p udp -s {router_public[1-index]} -j DROP; "
                    "iptables -P INPUT DROP; "
                    "iptables -P FORWARD DROP; "
                    f"iptables -A FORWARD -s {subnet} -d {transit_subnet} -j ACCEPT; "
                    "iptables -A FORWARD -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT; "
                    f"iptables -I FORWARD 1 -p udp -d {router_public[1-index]} -j DROP")
            relay_url = f"https://{relay_ip}"
            endpoint = {"identity_file": "/state/server.key", "relay_ca_file": "/fixtures/ca.pem",
                        "bind_addrs": ["0.0.0.0:0"], "relay_urls": [relay_url], "readiness_timeout_secs": timeout}
            server_config = {"version": 1, "daemon": {"node_id": "nat-server", "grpc_listen": "127.0.0.1:7789",
                "workspace": "/workspace", "auth": {"token": "owned-nat-token"}, "iroh": endpoint},
                "policy": {"subject": "nat-test", "fs": {"mounts": [{"name": "workspace", "path": "/",
                    "permissions": {"read": True, "write": True, "delete": True}}]}, "exec": {
                    "allowed_cwds": ["/"], "default_timeout_secs": 30, "max_timeout_secs": 300,
                    "preserve_env": False, "env_allowlist": [], "allowed_secrets": []}}}
            server_path = fixtures / "server.yaml"
            server_path.write_text(json.dumps(server_config))
            server_path.chmod(0o600)
            endpoints = []
            for index, role in enumerate(("server", "client")):
                cid = start(role, "--network", networks[index], "--tmpfs", "/state", "--tmpfs", "/workspace",
                    "--mount", f"type=bind,src={fixtures},dst=/fixtures,readonly", "--entrypoint", "sleep", native, "infinity")
                endpoints.append(cid)
                private_router = addresses(routers[index])[prefix + "-" + role]["IPAddress"]
                namespace(cid, f"ip route replace default via {private_router}")
            server, client = endpoints
            log_filter = "info,iroh::net_report=debug,iroh::socket::remote_map=debug,iroh::_events=debug"
            docker("exec", "-d", "-e", "RUST_LOG=" + log_filter, server, "sh", "-c", "operond start --config /fixtures/server.yaml >/state/daemon.log 2>&1")

            def identity():
                logs = re.sub(r"\x1b\[[0-9;]*m", "", docker("exec", server, "cat", "/state/daemon.log"))
                return re.search(r"endpoint_id=([a-f0-9]{64})", logs)
            wait(identity, lambda: docker("exec", server, "cat", "/state/daemon.log"))
            client_config = {"version": 1, "client": {"nodes": {"remote": {
                "endpoint": "iroh://" + identity()[1], "auth": {"token": "owned-nat-token"}, "transport": {
                    "connect_timeout_secs": timeout, "rpc_timeout_secs": timeout,
                    "iroh": dict(endpoint, identity_file="/state/client.key", peer_relay_urls=[relay_url])}}}}}
            (fixtures / "client.yaml").write_text(json.dumps(client_config))
            (fixtures / "client.yaml").chmod(0o600)
            docker("exec", "-d", "-e", f"OPERON_EMBEDDED_TEST_TIMEOUT_SECS={timeout}", "-e", "RUST_LOG=" + log_filter, client, "sh", "-c",
                f"nat_acceptance /fixtures/client.yaml {router_public[0]} >/state/observer.log 2>&1")
            wait(lambda: run("docker", "exec", client, "test", "-f", "/state/relay-ready", check=False).returncode == 0,
                 lambda: docker("exec", client, "cat", "/state/observer.log"))
            for index, router in enumerate(routers):
                namespace(router, f"iptables -D FORWARD -p udp -d {router_public[1-index]} -j DROP; "
                    f"iptables -t raw -D PREROUTING -p udp -s {router_public[1-index]} -j DROP")
            docker("exec", client, "touch", "/state/allow-direct")
            wait(lambda: run("docker", "exec", client, "test", "-f", "/state/direct-verified", check=False).returncode == 0,
                 lambda: docker("exec", client, "cat", "/state/observer.log"))
            payload = bytes(range(251)) * (8 * 1024 * 1024 // 251 + 1)
            (fixtures / "payload.bin").write_bytes(payload)

            def cli(*args):
                return docker("exec", client, "operon", "--config", "/fixtures/client.yaml", *args)
            assert "nat-exec-verified" in cli("exec", "run", "remote", "--cwd", "/", "printf nat-exec-verified")
            cli("fs", "write", "remote:/nat.bin", "--file", "/fixtures/payload.bin")
            cli("fs", "read", "remote:/nat.bin", "--output", "/state/result.bin")
            assert docker("exec", client, "sha256sum", "/state/result.bin").split()[0] == hashlib.sha256(payload).hexdigest()
            for router in routers:
                counters = namespace(router, "iptables -t nat -nvx -L POSTROUTING")
                assert re.search(r"\n\s*[1-9][0-9]*\s+[0-9]+\s+MASQUERADE\b", counters), counters
                print("PASS actual owned SNAT counter:", counters, flush=True)
            print(docker("exec", client, "cat", "/state/observer.log"), flush=True)
            print("PASS two private LANs/two SNAT routers, authenticated TLS relay bootstrap, selected public-IP upgrade, exec and >8MiB file SHA")
    except Exception:
        for cid in containers:
            name = docker("inspect", "--format", "{{.Name}}", cid)
            print("owned NAT diagnostic:", name, flush=True)
            if "-capture-" in name:
                print(docker("logs", cid), flush=True)
            for path in ("/state/observer.log", "/state/daemon.log", "/tmp/relay.log"):
                result = run("docker", "exec", cid, "cat", path, check=False)
                if result.returncode == 0:
                    print(result.stdout, flush=True)
            result = run("docker", "exec", cid, "iptables", "-nvx", "-L", "FORWARD", check=False)
            if result.returncode == 0:
                print(result.stdout, flush=True)
            result = run("docker", "exec", cid, "ip", "route", check=False)
            if result.returncode == 0:
                print(result.stdout, flush=True)
            result = run("docker", "exec", cid, "conntrack", "-L", "-p", "udp", check=False)
            if result.returncode == 0:
                print(result.stdout, flush=True)
        raise
    finally:
        for cid in reversed(containers):
            run("docker", "rm", "-f", cid, check=False)
        for network in reversed(networks):
            run("docker", "network", "rm", network, check=False)


if __name__ == "__main__":
    main()
