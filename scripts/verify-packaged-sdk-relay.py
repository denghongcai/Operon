#!/usr/bin/env python3
"""Actual downloaded SDK/daemon business acceptance over an owned Docker relay."""
import os
from pathlib import Path
import re
import subprocess
import time
import urllib.request
import uuid

root = Path(__file__).resolve().parents[1]
seconds = int(os.environ.get("OPERON_EMBEDDED_TEST_TIMEOUT_SECS", "180"))
if seconds <= 0:
    raise ValueError("test timeout must be positive")
for required in ("OPERON_NATIVE_TEST_DAEMON", "OPERON_SDK_NATIVE_PATH", "OPERON_SDK_TEST_JS_PATH"):
    if not Path(os.environ[required]).is_absolute():
        raise ValueError(required + " must identify an absolute extracted artifact")

image = os.environ.get("OPERON_RELAY_TEST_IMAGE")
owned_image = image is None
image = image or "operon-public-sdk-relay-" + uuid.uuid4().hex[:12]
container = None

def docker(*arguments):
    return subprocess.check_output(["docker", *arguments], text=True).strip()

try:
    if owned_image:
        subprocess.run(["docker", "build", "-t", image, "docker/iroh-relay"], cwd=root, check=True)
    container = docker("run", "-d", "--label", "operon.acceptance=public-sdk-relay",
                       "--publish", "127.0.0.1::3340", "--publish", "127.0.0.1::9090", image)
    if not re.fullmatch(r"[a-f0-9]{64}", container):
        container = None
        raise AssertionError("invalid owned relay ID")
    relay = docker("port", container, "3340/tcp")
    metrics = docker("port", container, "9090/tcp")
    assert re.fullmatch(r"127\.0\.0\.1:[0-9]+", relay), relay
    assert re.fullmatch(r"127\.0\.0\.1:[0-9]+", metrics), metrics
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    deadline = time.monotonic() + seconds
    while True:
        try:
            with opener.open("http://" + metrics + "/metrics", timeout=2) as response:
                assert response.status == 200
            break
        except OSError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.2)
    environment = dict(os.environ, OPERON_SDK_TEST_RELAY_URL="http://" + relay)
    subprocess.run(["node", "--expose-gc", "scripts/verify-sdk-native.mjs"], cwd=root,
                   env=environment, check=True, timeout=seconds)
    with opener.open("http://" + metrics + "/metrics", timeout=2) as response:
        counters = response.read().decode()
    for key in ("relayserver_bytes_sent_total", "relayserver_bytes_recv_total"):
        match = re.search(r"^" + key + r" ([0-9.e+]+)$", counters, re.M)
        assert match and float(match[1]) > 16 * 1024 * 1024, "insufficient actual public SDK relay traffic"
    print("PASS extracted public SDK/daemon relay-only business and byte counters")
finally:
    if container is not None:
        subprocess.run(["docker", "rm", "-f", container], check=False)
    if owned_image:
        subprocess.run(["docker", "image", "rm", image], check=False)
