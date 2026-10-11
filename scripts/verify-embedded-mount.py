#!/usr/bin/env python3
"""Owned Linux FUSE acceptance over native iroh; no TCP connectivity bridge."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import tempfile
import time


def main():
    timeout = int(os.environ.get("OPERON_EMBEDDED_TEST_TIMEOUT_SECS", "180"))
    if timeout <= 0:
        raise ValueError("test timeout must be positive")
    binaries = Path(os.environ.get("OPERON_NATIVE_BIN_DIR", "target/debug")).resolve()
    relay = os.environ.get("OPERON_MOUNT_TEST_RELAY_URL")
    with tempfile.TemporaryDirectory(prefix="operon-embedded-mount-") as temp:
        root = Path(temp)
        workspace, mountpoint = root / "workspace", root / "mount"
        workspace.mkdir()
        mountpoint.mkdir()
        with socket.socket() as tcp, socket.socket(type=socket.SOCK_DGRAM) as udp:
            tcp.bind(("127.0.0.1", 0))
            udp.bind(("127.0.0.1", 0))
            tcp_port, udp_port = tcp.getsockname()[1], udp.getsockname()[1]
        endpoint = {"identity_file": str(root / "server.key"), "bind_addrs": [f"127.0.0.1:{udp_port}"]}
        if relay:
            endpoint.update(bind_addrs=[], relay_urls=[relay], allow_insecure_relays=True, readiness_timeout_secs=timeout)
        config = {"version": 1, "daemon": {"node_id": "owned-mount", "grpc_listen": f"127.0.0.1:{tcp_port}",
            "workspace": str(workspace), "auth": {"token": "owned-mount-test-token"}, "iroh": endpoint},
            "policy": {"subject": "mount-acceptance", "exec": {"allowed_cwds": [], "default_timeout_secs": 30,
                "max_timeout_secs": 300, "preserve_env": False, "env_allowlist": [], "allowed_secrets": []}, "fs": {"mounts": [{"name": "workspace", "path": "/",
                "permissions": {"read": True, "write": True, "delete": True}}]}}}
        config_path = root / "config.yaml"
        config_path.write_text(json.dumps(config))
        config_path.chmod(0o600)
        daemon_log, mount_log = root / "daemon.log", root / "mount.log"
        daemon, mount = None, None
        try:
            with daemon_log.open("w") as log:
                daemon = subprocess.Popen([str(binaries / "operond"), "start", "--config", str(config_path)], stdout=log, stderr=log)
            deadline = time.monotonic() + timeout
            while True:
                logs = re.sub(r"\x1b\[[0-9;]*m", "", daemon_log.read_text())
                match = re.search(r"endpoint_id=([a-f0-9]{64})", logs)
                if match:
                    break
                if daemon.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("daemon readiness failed: " + daemon_log.read_text())
                time.sleep(0.1)
            client_transport = dict(endpoint, identity_file=str(root / "client.key"), bind_addrs=["127.0.0.1:0"], peer_addrs=[f"127.0.0.1:{udp_port}"])
            if relay:
                client_transport.update(bind_addrs=[], peer_addrs=[], peer_relay_urls=[relay])
            config["client"] = {"nodes": {"remote": {"endpoint": "iroh://" + match[1],
                "auth": {"token": "owned-mount-test-token"}, "transport": {"connect_timeout_secs": timeout, "rpc_timeout_secs": timeout, "iroh": client_transport}}}}
            config_path.write_text(json.dumps(config))
            config_path.chmod(0o600)
            with mount_log.open("w") as log:
                mount = subprocess.Popen([str(binaries / "operon"), "--config", str(config_path), "mount", "remote:/", "--to", str(mountpoint)], stdout=log, stderr=log)
            deadline = time.monotonic() + timeout
            while not os.path.ismount(mountpoint):
                if mount.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("mount readiness failed: " + mount_log.read_text())
                time.sleep(0.1)
            payload = bytes(range(251)) * (8 * 1024 * 1024 // 251 + 1)
            mounted = mountpoint / "large.bin"
            mounted.write_bytes(payload)
            assert hashlib.sha256(mounted.read_bytes()).digest() == hashlib.sha256(payload).digest()
            with mounted.open("rb", buffering=0) as handle:
                assert os.pread(handle.fileno(), 4 * 1024 * 1024, 123) == payload[123:123 + 4 * 1024 * 1024]
            renamed = mountpoint / "renamed.bin"
            mounted.rename(renamed)
            with renamed.open("r+b", buffering=0) as handle:
                handle.truncate(1024)
            assert renamed.read_bytes() == payload[:1024]
            assert (workspace / "renamed.bin").read_bytes() == payload[:1024]
            renamed.unlink()
            assert not (workspace / "renamed.bin").exists()
            subprocess.run(["fusermount3", "-uz", str(mountpoint)], check=True, timeout=timeout)
            assert mount.wait(timeout=timeout) == 0, mount_log.read_text()
            print("PASS native iroh FUSE large write/read/range/rename/truncate/delete and external lazy unmount" + (" via relay" if relay else " via direct path"))
        finally:
            if mount and mount.poll() is None:
                mount.send_signal(signal.SIGINT)
                try:
                    mount.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    mount.kill()
                    mount.wait()
            if os.path.ismount(mountpoint):
                subprocess.run(["fusermount3", "-uz", str(mountpoint)], check=True, timeout=timeout)
            if daemon and daemon.poll() is None:
                daemon.terminate()
                try:
                    daemon.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    daemon.kill()
                    daemon.wait()


if __name__ == "__main__":
    main()
