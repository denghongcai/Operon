#!/usr/bin/env python3
"""Actual packaged-binary Alpine runtime/live-FUSE acceptance (no skip gates)."""
import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--bin-dir', type=Path, required=True)
parser.add_argument('--live-mount', action='store_true')
parser.add_argument('--operations', type=int, default=500)
args = parser.parse_args()
assert args.operations > 0
bins = args.bin_dir.resolve()
timeout = float(os.environ.get('OPERON_ALPINE_ACCEPT_TIMEOUT_SECS', '60'))
stop_timeout = float(os.environ.get('OPERON_ALPINE_STOP_TIMEOUT_SECS', '30'))
assert timeout >= 0 and stop_timeout >= 0
processes, logs, threads = [], [], []
finished = threading.Event()


def wait_for(predicate, description):
    end = time.monotonic() + timeout if timeout else None
    while True:
        try:
            if predicate():
                return
        except OSError:
            # Explicit read/readiness probes may fail while a restarted peer
            # reconnects. This does not replay mutations in the product.
            pass
        if end is not None and time.monotonic() >= end:
            raise AssertionError('timed out: ' + description)
        time.sleep(0.05)


def port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def stop(process, signal=None):
    if process.poll() is None:
        if signal is None:
            process.terminate()
        else:
            process.send_signal(signal)
        try:
            process.wait(timeout=stop_timeout or None)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            raise AssertionError('process failed bounded graceful shutdown')


def spawn(command, path):
    log = path.open('wb')
    logs.append(log)
    process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
    processes.append(process)
    return process


def echo(sock, udp=False):
    sock.settimeout(0.1)
    while not finished.is_set():
        try:
            if udp:
                data, peer = sock.recvfrom(65507)
                sock.sendto(data, peer)
            else:
                connection, _ = sock.accept()
                with connection:
                    connection.settimeout(timeout or None)
                    data = connection.recv(65536)
                    connection.sendall(data)
        except socket.timeout:
            continue


with tempfile.TemporaryDirectory(prefix='operon-alpine-runtime-') as tmp:
    root = Path(tmp)
    workspace, mount = root / 'workspace', root / 'mount'
    workspace.mkdir()
    mount.mkdir()
    config = root / 'config.yaml'
    endpoint_port = port()
    token = os.urandom(24).hex()
    tcp = socket.socket()
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    tcp.bind(('127.0.0.1', 0))
    tcp.listen()
    udp.bind(('127.0.0.1', 0))
    policy_services = []
    for name, sock, protocol in [('tcp', tcp, 'tcp'), ('udp', udp, 'udp')]:
        policy_services.append(dict(id=name, name=name, host='127.0.0.1',
                                    port=sock.getsockname()[1], protocol=protocol,
                                    description='owned echo fixture',
                                    permissions=dict(check=True, forward=True)))
        thread = threading.Thread(target=echo, args=(sock, protocol == 'udp'), daemon=True)
        threads.append(thread)
        thread.start()
    content = dict(version=1,
                   daemon=dict(node_id='local', grpc_listen=f'127.0.0.1:{endpoint_port}',
                               workspace=str(workspace), store=str(root / 'store.jsonl'),
                               advertise_lan=False, auth=dict(token=token)),
                   client=dict(nodes=dict(local=dict(endpoint=f'grpc://127.0.0.1:{endpoint_port}',
                                                     auth=dict(token=token)))),
                   policy=dict(subject='alpine-acceptance',
                               fs=dict(mounts=[dict(name='workspace', path='/',
                                                  permissions=dict(read=True, write=True, delete=True))]),
                               exec=dict(allowed_cwds=['/'], default_timeout_secs=30,
                                         max_timeout_secs=300, allow_sessions=True, preserve_env=False,
                                         env_allowlist=[], allowed_secrets=[]),
                               service=dict(services=policy_services)))
    # JSON is a YAML subset, read by the existing YAML loader from config.yaml.
    config.write_text(json.dumps(content))
    config.chmod(0o600)

    def cli(*command, check=True, input=None, selected=config):
        result = subprocess.run([str(bins / 'operon'), '--config', str(selected), *command],
                                input=input, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                timeout=timeout or None)
        if check and result.returncode:
            raise AssertionError((command, result.returncode, result.stderr.decode(errors='replace')))
        return result

    def ready():
        return cli('node', 'ping', 'local', check=False).returncode == 0

    def daemon():
        process = spawn([str(bins / 'operond'), 'start', '--config', str(config)], root / 'daemon.log')
        wait_for(ready, 'daemon readiness')
        return process

    mounted = False
    try:
        server = daemon()
        cli('doctor')
        bad = json.loads(json.dumps(content))
        bad['client']['nodes']['local']['auth']['token'] = 'deliberately-invalid'
        bad_config = root / 'bad.yaml'
        bad_config.write_text(json.dumps(bad))
        bad_config.chmod(0o600)
        rejected = cli('node', 'ping', 'local', selected=bad_config, check=False)
        assert rejected.returncode != 0 and b'invalid bearer token' in rejected.stderr.lower(), (
            rejected.returncode, rejected.stdout, rejected.stderr)
        result = cli('exec', 'session', 'local', '--timeout-secs', '30', '--argv', '--',
                     '/bin/sh', '-lc', 'printf alpine-pty-ok', input=b'')
        assert b'alpine-pty-ok' in result.stdout
        assert cli('fs', 'stat', 'local:/../escape', check=False).returncode != 0
        cli('fs', 'write', 'local:/survivor', '--content', 'ORIGINAL')
        assert cli('fs', 'write', 'local:/survivor', '--file', str(root), check=False).returncode != 0
        assert cli('fs', 'read', 'local:/survivor').stdout == b'ORIGINAL'

        for protocol in ('tcp', 'udp'):
            forwarded_port = port()
            process = spawn([str(bins / 'operon'), '--config', str(config), 'service',
                             'forward' if protocol == 'tcp' else 'forward-udp', 'local', protocol,
                             '--listen', f'127.0.0.1:{forwarded_port}'], root / f'{protocol}.log')
            payload = b'alpine-forward-' + protocol.encode()

            def forwarded():
                try:
                    with socket.socket(socket.AF_INET, socket.SOCK_STREAM if protocol == 'tcp'
                                       else socket.SOCK_DGRAM) as peer:
                        peer.settimeout(min(timeout, 0.5) if timeout else 0.5)
                        peer.connect(('127.0.0.1', forwarded_port))
                        peer.sendall(payload)
                        return peer.recv(65536) == payload
                except OSError:
                    return False

            wait_for(forwarded, protocol + ' actual forwarding')
            stop(process)

        if args.live_mount:
            assert Path('/dev/fuse').exists(), 'required /dev/fuse is missing (not a skip)'
            helper = shutil.which('fusermount3') or shutil.which('fusermount')
            assert helper, 'required non-root mount helper is missing (not a skip)'
            cli('doctor', '--mount-runtime')
            process = spawn([str(bins / 'operon'), '--config', str(config), 'mount',
                             'local:/', '--to', str(mount)], root / 'mount.log')
            wait_for(lambda: os.path.ismount(mount), 'real FUSE mount')
            mounted = True
            assert (mount / 'survivor').read_bytes() == b'ORIGINAL'
            def expect_errno(expected, operation):
                try:
                    operation()
                except OSError as error:
                    assert error.errno == expected, (expected, error)
                else:
                    raise AssertionError('expected FUSE errno', expected)

            expect_errno(errno.ENOENT, lambda: (mount / 'absent').read_bytes())
            directory = mount / 'errno-directory'
            directory.mkdir()
            expect_errno(errno.EEXIST, directory.mkdir)
            (directory / 'child').write_bytes(b'errno-integrity')
            expect_errno(errno.ENOTEMPTY, directory.rmdir)
            assert (directory / 'child').read_bytes() == b'errno-integrity'
            (directory / 'child').unlink()
            directory.rmdir()
            start = time.monotonic()
            for i in range(args.operations):
                path = mount / f'item-{i}'
                path.write_bytes(b'alpine-' + str(i).encode())
                assert path.read_bytes() == b'alpine-' + str(i).encode()
                path.unlink()
            seconds = time.monotonic() - start
            data = os.urandom(8 * 1024 * 1024)
            (mount / 'binary').write_bytes(data)
            assert hashlib.sha256((mount / 'binary').read_bytes()).digest() == hashlib.sha256(data).digest()
            with (mount / 'binary').open('r+b', buffering=0) as handle:
                handle.seek(1024 * 1024 + 17)
                handle.write(b'changed')
                handle.truncate(2 * 1024 * 1024)
            expected = bytearray(data[:2 * 1024 * 1024])
            expected[1024 * 1024 + 17:1024 * 1024 + 24] = b'changed'
            assert (mount / 'binary').read_bytes() == expected
            (mount / 'binary').rename(mount / 'renamed')
            assert not (mount / 'binary').exists() and (mount / 'renamed').read_bytes() == expected
            stop(server)
            assert cli('node', 'ping', 'local', check=False).returncode != 0
            server = daemon()
            wait_for(lambda: (mount / 'renamed').read_bytes() == expected, 'mount recovery after daemon restart')
            subprocess.run([helper, '-u', str(mount)], check=True, timeout=timeout or None)
            mounted = False
            stop(process)
            assert not os.path.ismount(mount)
            print(json.dumps(dict(test='fuse-create-read-delete', operations=args.operations,
                                  ops_per_second=args.operations / seconds, seconds=seconds,
                                  uid=os.getuid(), architecture=os.uname().machine)))

        events = json.loads(cli('--json', 'audit', 'show', 'local', '--limit', '10').stdout)['events']
        assert events
        stop(server)
        server = daemon()
        assert cli('fs', 'read', 'local:/survivor').stdout == b'ORIGINAL'
        assert json.loads(cli('--json', 'audit', 'show', 'local', '--limit', '10').stdout)['events']
        old = root / 'old-workspace'
        workspace.rename(old)
        workspace.mkdir()
        assert cli('fs', 'stat', 'local:/', check=False).returncode != 0
        workspace.rmdir()
        old.rename(workspace)
        print('PASS: real Alpine auth/policy/PTY/TCP/UDP/source-error/restart/root-identity' +
              ('/live-FUSE' if args.live_mount else ' (live-FUSE gate runs separately)'))
    except BaseException:
        for path in root.glob('*.log'):
            print(path.name + ':\n' + path.read_text(errors='replace'))
        raise
    finally:
        if mounted:
            helper = shutil.which('fusermount3') or shutil.which('fusermount')
            subprocess.run([helper, '-u', '-z', str(mount)], check=False, timeout=timeout or None)
        for process in reversed(processes):
            stop(process)
        for log in logs:
            log.close()
        finished.set()
        for thread in threads:
            thread.join(timeout=stop_timeout or None)
        tcp.close()
        udp.close()
