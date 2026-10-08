#!/usr/bin/env python3
"""Native GNU/musl comparison in one identical GNU runtime; ops/s is primary.

No LD_PRELOAD instrumentation: fully static musl does not participate in dynamic
loader interposition. Fresh private fixtures, integrity assertions and processes
are identical for both flavors; no parallel-read tuning is performed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import statistics
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--gnu-bin-dir', type=Path, required=True)
parser.add_argument('--musl-bin-dir', type=Path, required=True)
parser.add_argument('--operations', type=int, default=200)
parser.add_argument('--exec-operations', type=int, default=30)
parser.add_argument('--repeats', type=int, default=3)
args = parser.parse_args()
assert args.operations > 0 and args.exec_operations > 0 and args.repeats > 0
timeout = float(os.environ.get('OPERON_LIBC_BENCH_TIMEOUT_SECS', '120'))
stop_timeout = float(os.environ.get('OPERON_LIBC_BENCH_STOP_TIMEOUT_SECS', '30'))
assert timeout >= 0 and stop_timeout >= 0
assert Path('/dev/fuse').is_char_device(), 'required live FUSE cannot be skipped'
payload = bytes(range(251)) * 16710
digest = hashlib.sha256(payload).hexdigest()
rows = []
versions = {}


def run(command, **kwargs):
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            timeout=timeout or None, **kwargs)
    assert result.returncode == 0, (command, result.stderr.decode(errors='replace'))
    return result


def wait(predicate, description):
    deadline = time.monotonic() + timeout if timeout else None
    while not predicate():
        if deadline is not None and time.monotonic() >= deadline:
            raise AssertionError('timed out: ' + description)
        time.sleep(0.02)


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=stop_timeout or None)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            raise AssertionError('benchmark process failed graceful shutdown')


def measure(flavor, trial, name, operations, function):
    start = time.perf_counter()
    function()
    seconds = time.perf_counter() - start
    row = dict(flavor=flavor, trial=trial, test=name, operations=operations,
               ops_per_second=operations / seconds, seconds=seconds)
    rows.append(row)
    print(json.dumps(row), flush=True)


def case(flavor, binaries, trial):
    binaries = binaries.resolve()
    version = run([str(binaries / 'operon'), '--version']).stdout.decode().strip()
    versions[flavor] = version
    with tempfile.TemporaryDirectory(prefix='operon-libc-benchmark-') as tmp:
        root = Path(tmp)
        workspace, mount = root / 'workspace', root / 'mount'
        workspace.mkdir()
        mount.mkdir()
        config = root / 'config.yaml'
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        token = os.urandom(24).hex()
        config.write_text(json.dumps(dict(version=1,
            daemon=dict(node_id='local', grpc_listen=f'127.0.0.1:{port}', workspace=str(workspace),
                        store=str(root / 'store.jsonl'), advertise_lan=False, auth=dict(token=token)),
            client=dict(nodes=dict(local=dict(endpoint=f'grpc://127.0.0.1:{port}', auth=dict(token=token)))),
            policy=dict(subject='libc-benchmark',
                        fs=dict(mounts=[dict(name='workspace', path='/', permissions=dict(read=True, write=True, delete=True))]),
                        exec=dict(allowed_cwds=['/'], default_timeout_secs=30, max_timeout_secs=300,
                                  allow_sessions=True, preserve_env=False, env_allowlist=[], allowed_secrets=[])))))
        config.chmod(0o600)
        command = [str(binaries / 'operon'), '--config', str(config)]
        daemon_log, mount_log = (root / 'daemon.log').open('wb'), (root / 'mount.log').open('wb')
        daemon = subprocess.Popen([str(binaries / 'operond'), 'start', '--config', str(config)], stdout=daemon_log, stderr=subprocess.STDOUT)
        mounted = None
        try:
            def ready():
                return subprocess.run([*command, 'node', 'ping', 'local'], stdout=subprocess.DEVNULL,
                                      stderr=subprocess.DEVNULL, timeout=timeout or None).returncode == 0
            wait(ready, 'daemon readiness')
            (workspace / 'stat-target').write_bytes(b'stat-integrity')
            def stats():
                for _ in range(args.operations):
                    response = json.loads(run([*command, '--json', 'fs', 'stat', 'local:/stat-target']).stdout)
                    assert response['size'] == len(b'stat-integrity'), response
            measure(flavor, trial, 'cli/fs-stat', args.operations, stats)
            def execs():
                for _ in range(args.exec_operations):
                    response = json.loads(run([*command, '--json', 'exec', 'run', 'local', '--', 'printf exec-integrity > exec-marker']).stdout)
                    assert response['status'] == 'succeeded' and response['exit_code'] == 0, response
                    assert (workspace / 'exec-marker').read_bytes() == b'exec-integrity'
            measure(flavor, trial, 'cli/exec-complete', args.exec_operations, execs)
            mounted = subprocess.Popen([*command, 'mount', 'local:/', '--to', str(mount)], stdout=mount_log, stderr=subprocess.STDOUT)
            wait(lambda: os.path.ismount(mount), 'real FUSE mount')
            def files():
                for index in range(args.operations):
                    path = mount / f'file-{index}'
                    path.write_bytes(b'file-integrity')
                    assert path.read_bytes() == b'file-integrity'
                    path.unlink()
            measure(flavor, trial, 'fuse/create-read-delete', args.operations, files)
            source = root / 'payload'
            source.write_bytes(payload)
            rss = {}
            def transfer(mode):
                arguments = ['fs', 'write', 'local:/payload', '--file', str(source)] if mode == 'write' else ['fs', 'read', 'local:/payload', '--output', str(root / 'received')]
                evidence = root / f'{mode}.rss'
                run(['/usr/bin/time', '-f', '%M', '-o', str(evidence), *command, *arguments])
                rss[mode] = int(evidence.read_text().strip())
                result = workspace / 'payload' if mode == 'write' else root / 'received'
                assert hashlib.sha256(result.read_bytes()).hexdigest() == digest
            measure(flavor, trial, 'cli/write-stream-integrity', 1, lambda: transfer('write'))
            measure(flavor, trial, 'cli/read-stream-integrity', 1, lambda: transfer('read'))
            status = Path(f'/proc/{daemon.pid}/status').read_text()
            hwm = next(line.split()[1] for line in status.splitlines() if line.startswith('VmHWM:'))
            print(json.dumps(dict(flavor=flavor, trial=trial, auxiliary=True,
                                  daemon_rss_hwm_kib=int(hwm), cli_transfer_rss_kib=rss,
                                  binary_sha256={name: hashlib.sha256((binaries / name).read_bytes()).hexdigest() for name in ['operon', 'operond']})), flush=True)
        except Exception:
            for log in [root / 'daemon.log', root / 'mount.log']:
                print(log.read_text(errors='replace')[-10000:])
            raise
        finally:
            cleanup_errors = []
            if os.path.ismount(mount):
                try:
                    run(['fusermount3', '-u', str(mount)])
                except Exception as error:
                    cleanup_errors.append(error)
            for process in [mounted, daemon]:
                if process is not None:
                    try:
                        stop(process)
                    except Exception as error:
                        cleanup_errors.append(error)
            daemon_log.close()
            mount_log.close()
            if cleanup_errors:
                raise AssertionError('benchmark cleanup failed', cleanup_errors)
        assert not os.path.ismount(mount)


# Alternate order to reduce warm-cache/order effects; each case has fresh state.
for trial in range(args.repeats):
    flavors = [('gnu', args.gnu_bin_dir), ('musl', args.musl_bin_dir)]
    if trial % 2:
        flavors.reverse()
    for flavor, directory in flavors:
        case(flavor, directory, trial)
assert versions['gnu'] == versions['musl'], 'comparison binaries must share a package version'
for name in sorted({row['test'] for row in rows}):
    medians = {flavor: statistics.median(row['ops_per_second'] for row in rows if row['test'] == name and row['flavor'] == flavor) for flavor in ['gnu', 'musl']}
    print(json.dumps(dict(summary=True, test=name, median_ops_per_second=medians,
                          musl_over_gnu=medians['musl'] / medians['gnu'], architecture=os.uname().machine)), flush=True)
print('PASS: native identical-environment libc comparison; ops/s primary, time/RSS auxiliary', flush=True)
