#!/usr/bin/env python3
"""Isolated matched release-binary FUSE read trials; no host cache flushing.

One sequential operation is a whole verified file, one random operation is a
verified 4KiB pread. Fresh mount per case; server page cache remains warm.
Requires Docker and an image containing python3, fuse3 and (for delay) tc.
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import random
import secrets
import shutil
import statistics
import subprocess
import tempfile
import time


def worker(args):
    root = Path('/review')
    mountpoint = Path('/mnt/remote')
    mountpoint.mkdir(parents=True, exist_ok=True)
    rows = []
    pattern = bytes(range(251))
    def audit_count():
        path = root / 'store.jsonl'
        if not path.exists():
            return 0
        return sum(json.loads(line).get('event', {}).get('action') == 'read-range'
                   for line in path.read_text().splitlines() if line.strip())
    def network():
        fields = next(line for line in Path('/proc/net/dev').read_text().splitlines()
                      if 'eth0:' in line).split(':')[1].split()
        return int(fields[0]), int(fields[8])
    for count in args.readers:
        for mode in ['same', 'distinct', 'random']:
            command = ['/review/bin/operon', '--config', '/review/config.yaml',
                       'mount', 'local:/', '--to', str(mountpoint), *args.mount_args]
            with open(root / 'mount.log', 'a') as log:
                process = subprocess.Popen(command, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 30
                while not os.path.ismount(mountpoint):
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError((root / 'mount.log').read_text())
                    time.sleep(.05)
                def read(index):
                    path = mountpoint / f'file-{index if mode == "distinct" else 0}'
                    start = time.perf_counter()
                    with open(path, 'rb', buffering=0) as source:
                        if mode == 'random':
                            rng = random.Random(index)
                            for _ in range(args.operations):
                                offset = rng.randrange(args.mib * 256) * 4096
                                expected = (pattern * 18)[offset % 251:offset % 251 + 4096]
                                assert os.pread(source.fileno(), 4096, offset) == expected
                            operations, size = args.operations, args.operations * 4096
                        else:
                            digest = hashlib.sha256()
                            size = 0
                            while chunk := source.read(1024 * 1024):
                                digest.update(chunk)
                                size += len(chunk)
                            assert size == args.mib * 1024 * 1024
                            assert digest.hexdigest() == args.digest
                            operations = 1
                    return operations, size, time.perf_counter() - start
                before_audit = audit_count()
                before_rx, before_tx = network()
                start = time.perf_counter()
                with concurrent.futures.ThreadPoolExecutor(max_workers=count) as pool:
                    results = list(pool.map(read, range(count)))
                elapsed = time.perf_counter() - start
                after_rx, after_tx = network()
                operations = sum(result[0] for result in results)
                row = dict(mode=mode, readers=count, operations=operations,
                           bytes=sum(result[1] for result in results), seconds=elapsed,
                           ops_per_second=operations / elapsed,
                           read_rpcs=audit_count() - before_audit,
                           rx_bytes=after_rx - before_rx, tx_bytes=after_tx - before_tx,
                           reader_seconds=[result[2] for result in results])
                status = Path(f'/proc/{process.pid}/status').read_text()
                row['mount_peak_rss_kib'] = int(next(line for line in status.splitlines()
                                                   if line.startswith('VmHWM:')).split()[1])
                rows.append(row)
                print(json.dumps(row), flush=True)
            finally:
                if os.path.ismount(mountpoint):
                    subprocess.run(['fusermount3', '-u', str(mountpoint)], check=True)
                if process.poll() is None:
                    process.terminate()
                process.wait(timeout=10)
    (root / 'reads.json').write_text(json.dumps(rows, indent=2))


def main(args):
    repo = Path(__file__).resolve().parents[2]
    output = args.output.resolve() if args.output else Path(tempfile.mkdtemp(prefix='operon-parallel-'))
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError('Output must be empty; existing evidence is never overwritten')
    prefix = 'operon-read-' + secrets.token_hex(6)
    containers = []
    def docker(*command, check=True):
        result = subprocess.run(['docker', *map(str, command)], capture_output=True, text=True)
        if check and result.returncode:
            raise RuntimeError(f'Docker {command[0]} failed: {result.stderr or result.stdout}')
        return result.stdout
    environment = dict(head=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
                       dirty=subprocess.check_output(['git', 'status', '--short'], text=True),
                       image=args.image, mib=args.mib, delay_ms=args.delay_ms,
                       operations=args.operations, readers=args.readers, repeats=args.repeats)
    environment['image_id'] = docker('image', 'inspect', '--format', '{{.Id}}', args.image).strip()
    (output / 'environment.json').write_text(json.dumps(environment, indent=2))
    try:
        docker('network', 'create', prefix)
        for trial in range(args.repeats):
            variants = [('baseline', args.baseline), ('candidate', args.candidate)]
            if trial % 2:
                variants.reverse()
            for label, binaries in variants:
                if binaries is None:
                    continue
                case = output / f'{label}-{trial}'
                (case / 'bin').mkdir(parents=True)
                (case / 'workspace').mkdir()
                hashes = {}
                for name in ['operon', 'operond']:
                    shutil.copy2(binaries / name, case / 'bin' / name)
                    hashes[name] = hashlib.sha256((case / 'bin' / name).read_bytes()).hexdigest()
                size = args.mib * 1024 * 1024
                payload = (bytes(range(251)) * ((size + 250) // 251))[:size]
                digest = hashlib.sha256(payload).hexdigest()
                for index in range(max(args.readers)):
                    (case / 'workspace' / f'file-{index}').write_bytes(payload)
                del payload
                (case / 'token').write_text(secrets.token_hex(32))
                (case / 'token').chmod(0o600)
                (case / 'config.yaml').write_text('''version: 1
daemon:
  node_id: local
  grpc_listen: 0.0.0.0:7789
  workspace: /review/workspace
  advertise_lan: false
  store: /review/store.jsonl
  auth: {token_file: /review/token}
client:
  nodes:
    local:
      endpoint: grpc://daemon:7789
      auth: {token_file: /review/token}
policy:
  subject: benchmark
  fs:
    mounts:
      - name: workspace
        path: /
        permissions: {read: true, write: true, delete: true}
  exec:
    allowed_cwds: [/]
    default_timeout_secs: 30
    max_timeout_secs: 60
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
  service: {services: []}
''')
                daemon, client = prefix + '-daemon', prefix + '-client'
                for name in [daemon, client]:
                    docker('run', '-d', '--name', name, '--network', prefix,
                           '--network-alias', 'daemon' if name == daemon else 'client',
                           '--device', '/dev/fuse', '--cap-add', 'SYS_ADMIN', '--cap-add', 'NET_ADMIN',
                           '-v', f'{case}:/review', '-v', f'{repo}:/repo:ro',
                           args.image, 'sleep', 'infinity')
                    containers.append(name)
                docker('exec', '-d', daemon, 'sh', '-c',
                       'exec /review/bin/operond start --config /review/config.yaml > /review/daemon.log 2>&1')
                for _ in range(200):
                    result = subprocess.run(['docker', 'exec', client, '/review/bin/operon',
                                             '--config', '/review/config.yaml', 'node', 'ping', 'local'],
                                            capture_output=True)
                    if result.returncode == 0:
                        break
                    time.sleep(.05)
                else:
                    raise RuntimeError('Daemon readiness failed: ' + result.stderr.decode(errors='replace'))
                if args.delay_ms:
                    docker('exec', client, 'sh', '-c',
                           'command -v tc >/dev/null || (apt-get update -qq && apt-get install -y -qq iproute2)')
                    docker('exec', client, 'tc', 'qdisc', 'add', 'dev', 'eth0', 'root',
                           'netem', 'delay', f'{args.delay_ms}ms')
                command = ['exec', client, 'python3', '/repo/scripts/performance/parallel-read.py',
                           '--worker', '--mib', str(args.mib), '--digest', digest,
                           '--operations', str(args.operations), '--readers', *map(str, args.readers)]
                if label == 'candidate' and args.mount_args:
                    command += ['--mount-args', *args.mount_args]
                (case / 'stdout.jsonl').write_text(docker(*command))
                resources = {name: docker('exec', daemon, 'cat', '/sys/fs/cgroup/' + name)
                             for name in ['cpu.stat', 'memory.peak']}
                # Copy private audit evidence without changing its permissions.
                docker('cp', f'{daemon}:/review/store.jsonl', case / 'store-export.jsonl')
                actions = {}
                for line in (case / 'store-export.jsonl').read_text().splitlines():
                    record = json.loads(line)
                    if record.get('kind') == 'audit':
                        action = record['event']['action']
                        actions[action] = actions.get(action, 0) + 1
                (case / 'evidence.json').write_text(json.dumps(dict(
                    binary_sha256=hashes, audit_actions=actions, resources=resources), indent=2))
                for name in [client, daemon]:
                    docker('rm', '-f', name)
                    containers.remove(name)
                # Recoverable compression of this case's verified owned payloads;
                # never touch unrelated files or discard benchmark evidence.
                subprocess.run(['gzip', '--', *[str(case / 'workspace' / f'file-{index}')
                                for index in range(max(args.readers))]], check=True)
                print(f'{label}-{trial}: {case}', flush=True)
        summary = {}
        for label in ['baseline', 'candidate']:
            values = {}
            for path in output.glob(f'{label}-*/reads.json'):
                for row in json.loads(path.read_text()):
                    key = f'{row["mode"]}/{row["readers"]}'
                    values.setdefault(key, []).append(row['ops_per_second'])
            summary[label] = {key: dict(trials=trials, median=statistics.median(trials))
                              for key, trials in values.items()}
        (output / 'summary.json').write_text(json.dumps(summary, indent=2))
        print(json.dumps(summary, indent=2))
    finally:
        for name in reversed(containers):
            docker('rm', '-f', name, check=False)
        docker('network', 'rm', prefix, check=False)
        print(f'Evidence retained: {output}', flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worker', action='store_true')
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--candidate', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--image', default='operon-review-tools:latest')
    parser.add_argument('--mib', type=int, default=64)
    parser.add_argument('--operations', type=int, default=100)
    parser.add_argument('--readers', type=int, nargs='+', default=[1, 2, 4, 8, 16])
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--delay-ms', type=int, default=0)
    parser.add_argument('--digest')
    parser.add_argument('--mount-args', nargs=argparse.REMAINDER, default=[])
    args = parser.parse_args()
    if min(args.mib, args.operations, args.repeats, *args.readers) <= 0 or args.delay_ms < 0:
        parser.error('Sizes, operations, repeats and readers must be positive; delay nonnegative')
    if args.worker:
        worker(args)
    elif args.baseline or args.candidate:
        main(args)
    else:
        parser.error('Provide --baseline and/or --candidate')
