#!/usr/bin/env python3
"""Opt-in native mount fixture pairing; leaves default gRPC smoke unchanged."""
import argparse
import os
from pathlib import Path
import re
import time

PLACEHOLDER = "iroh://" + "0" * 64


def setup(text):
    if text.count("\ndaemon:\n") != 1:
        raise ValueError("expected exactly one daemon fixture")
    text = text.replace("\ndaemon:\n", "\ndaemon:\n  iroh:\n    identity_file: server.key\n    bind_addrs: [127.0.0.1:0]\n", 1)
    text, count = re.subn(r"(?m)^      endpoint: grpc://127\.0\.0\.1:[0-9]+$",
        "      endpoint: " + PLACEHOLDER + "\n      transport:\n        iroh:\n          identity_file: client.key\n          bind_addrs: [127.0.0.1:0]\n          peer_addrs: [127.0.0.1:1]", text)
    if count != 1:
        raise ValueError("expected exactly one direct client fixture")
    return text


def pair(text, logs):
    logs = re.sub(r"\x1b\[[0-9;]*m", "", logs)
    identity = re.search(r"endpoint_id=([a-f0-9]{64})", logs)
    address = re.search(r"Ip\((127\.0\.0\.1:[0-9]+)\)", logs)
    if not identity or not address:
        return None
    if text.count(PLACEHOLDER) != 1 or text.count("peer_addrs: [127.0.0.1:1]") != 1:
        raise ValueError("fixture pairing placeholders missing or ambiguous")
    return text.replace(PLACEHOLDER, "iroh://" + identity[1]).replace("peer_addrs: [127.0.0.1:1]", "peer_addrs: [" + address[1] + "]")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("setup", "pair", "self-test"))
    parser.add_argument("config", nargs="?")
    parser.add_argument("logs", nargs="*")
    args = parser.parse_args()
    if args.action == "self-test":
        original = "version: 1\ndaemon:\n  auth:\n    token: test\nclient:\n  nodes:\n    remote:\n      endpoint: grpc://127.0.0.1:18841\n"
        prepared = setup(original)
        assert "identity_file: server.key" in prepared and "token: test" in prepared
        assert pair(prepared, "not ready") is None
        logs = "\x1b[3mendpoint_id\x1b[0m\x1b[2m=\x1b[0m" + "ab" * 32 + " addresses={Ip(127.0.0.1:45001)}"
        paired = pair(prepared, logs)
        assert paired and "iroh://" + "ab" * 32 in paired and "peer_addrs: [127.0.0.1:45001]" in paired
        for bad in ("", prepared):
            try:
                setup(bad)
            except ValueError:
                continue
            raise AssertionError("ambiguous/missing fixture accepted")
        print("PASS embedded mount fixture setup, explicit pairing and styled-log parsing")
        return
    if not args.config:
        parser.error("config is required")
    config = Path(args.config)
    text = config.read_text()
    if args.action == "setup":
        config.write_text(setup(text))
        return
    if not args.logs:
        parser.error("daemon logs are required for pairing")
    timeout = int(os.environ.get("OPERON_EMBEDDED_TEST_TIMEOUT_SECS", "180"))
    if timeout <= 0:
        raise ValueError("pairing timeout must be positive")
    deadline = time.monotonic() + timeout
    while True:
        logs = "\n".join(Path(path).read_text(errors="replace") for path in args.logs if Path(path).exists())
        paired = pair(text, logs)
        if paired:
            config.write_text(paired)
            print("PASS explicit native iroh fixture pairing; no TCP fallback")
            return
        if time.monotonic() >= deadline:
            raise RuntimeError("iroh listener pairing timed out: " + logs)
        time.sleep(0.1)


if __name__ == "__main__":
    main()
