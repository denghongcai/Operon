#!/usr/bin/env python3
"""Real booted Alpine/OpenRC system-service acceptance; never touches the host."""
import argparse
import json
import os
from pathlib import Path
import signal
import shlex
import socket
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--bin-dir', type=Path, required=True)
parser.add_argument('--after-reboot', action='store_true')
args = parser.parse_args()
bins = args.bin_dir.resolve()
timeout = float(os.environ.get('OPERON_OPENRC_ACCEPT_TIMEOUT_SECS', '120'))
stop_timeout = int(os.environ.get('OPERON_OPENRC_STOP_TIMEOUT_SECS', '30'))
assert timeout >= 0 and stop_timeout >= 0
assert os.getuid() == 0
assert Path('/proc/1/comm').read_text().strip() == 'init', 'real Alpine PID 1 is required'
assert Path('/run/openrc/softlevel').read_text().strip() == 'default', 'real OpenRC boot is required'
print(subprocess.check_output(['apk', 'info', '-v', 'openrc', 'python3'], timeout=timeout or None).decode().strip(), flush=True)


def run(*command, check=True, **kwargs):
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            timeout=timeout or None, **kwargs)
    if check and result.returncode:
        raise AssertionError((command, result.returncode, result.stdout.decode(errors='replace'),
                              result.stderr.decode(errors='replace')))
    return result


def wait(predicate, description):
    deadline = time.monotonic() + timeout if timeout else None
    while not predicate():
        if deadline is not None and time.monotonic() >= deadline:
            raise AssertionError('timed out: ' + description)
        time.sleep(0.05)


def service(*command, check=True, backend='openrc'):
    return run(str(bins / 'operond'), 'service', '--backend', backend,
               '--timeout-secs', str(int(timeout)), '--stop-timeout-secs', str(stop_timeout),
               *command, check=check)


def status():
    return json.loads(service('status', '--json').stdout)['details']


def daemon_pids():
    found = []
    for path in Path('/proc').iterdir():
        if not path.name.isdigit():
            continue
        try:
            command = (path / 'cmdline').read_bytes().split(b'\0')
            if command[:2] == [os.fsencode(bins / 'operond'), b'start']:
                found.append(int(path.name))
        except (FileNotFoundError, ProcessLookupError):
            pass
    return found


if not args.after_reboot:
    run('adduser', '-D', '-u', '1000', 'operon-test')
root = Path('/home/operon-test') / "Operon space '$value'"
root.mkdir(mode=0o700, exist_ok=args.after_reboot)
os.chown(root, 1000, 1000)
workspace = root / 'workspace'
workspace.mkdir(mode=0o700, exist_ok=args.after_reboot)
os.chown(workspace, 1000, 1000)
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    port = sock.getsockname()[1]
token = os.urandom(24).hex()
config = root / "config '$name'.yaml"
content = dict(version=1,
               daemon=dict(node_id='local', grpc_listen=f'127.0.0.1:{port}', workspace=str(workspace),
                           store=str(root / 'store.jsonl'), advertise_lan=False, auth=dict(token=token)),
               client=dict(nodes=dict(local=dict(endpoint=f'grpc://127.0.0.1:{port}', auth=dict(token=token)))),
               policy=dict(subject='openrc-acceptance',
                           fs=dict(mounts=[dict(name='workspace', path='/',
                                              permissions=dict(read=True, write=True, delete=True))]),
                           exec=dict(allowed_cwds=['/'], default_timeout_secs=30, max_timeout_secs=300,
                                     allow_sessions=True, preserve_env=False, env_allowlist=[], allowed_secrets=[])))
if not args.after_reboot:
    config.write_text(json.dumps(content))
    config.chmod(0o600)
    os.chown(config, 1000, 1000)


def cli(*command, check=True):
    return run(str(bins / 'operon'), '--config', str(config), *command, check=check)


def install(*extra):
    return service('install', '--config', str(config), '--service-user', 'operon-test', *extra)


installed = args.after_reboot
try:
    if args.after_reboot:
        wait(lambda: status()['healthy'], 'real reboot OpenRC boot-enabled readiness')
        assert Path(f'/proc/{daemon_pids()[0]}').stat().st_uid == 1000
        assert cli('fs', 'read', 'local:/persist.txt').stdout == b'openrc-persist'
        service('stop', '--stop-timeout-secs', '0')
        install('--respawn-delay-secs', '0', '--stop-timeout-secs', '0', '--shutdown-timeout-secs', '0')
        assert not daemon_pids()
        service('start', '--timeout-secs', '0')
        assert status()['healthy']
        other = Path('/etc/runlevels/operon-fixture-other')
        other.mkdir()
        other_entry = other / 'operond'
        other_entry.symlink_to('/etc/init.d/operond')
        assert service('uninstall', check=False).returncode != 0
        assert other_entry.is_symlink() and status()['healthy']
        other_entry.unlink()
        other.rmdir()
        service('uninstall')
        installed = False
        service('uninstall')
        assert not status()['installed']
        assert not daemon_pids()
        assert not Path('/etc/init.d/operond').exists()
        assert not Path('/etc/runlevels/default/operond').exists()
        assert not Path('/etc/operon/openrc-service.json').exists()
        assert config.exists() and (root / 'store.jsonl').exists()
        assert (workspace / 'persist.txt').read_text() == 'openrc-persist'
        assert config.stat().st_mode & 0o777 == 0o600
        assert config.stat().st_uid == 1000
        print(json.dumps(dict(test='openrc-real-lifecycle', architecture=os.uname().machine,
                              uid=1000, init=Path('/proc/1/comm').read_text().strip(),
                              alpine=Path('/etc/alpine-release').read_text().strip())))
        print('PASS: rebooted OpenRC identity/permissions/quoting/install/start/readiness/status/crash/boot/stop/reinstall/uninstall')
        raise SystemExit(0)
    assert service('install', '--config', str(config), check=False).returncode != 0
    assert service('install', '--config', str(config), '--service-user', 'root', check=False).returncode != 0
    assert service('install', '--config', str(config), '--service-user', 'missing-operon-user', check=False).returncode != 0
    config.chmod(0o644)
    assert service('install', '--config', str(config), '--service-user', 'operon-test', check=False).returncode != 0
    config.chmod(0o600)
    conflict = Path('/etc/init.d/operond')
    conflict.write_text('# unrelated service\n')
    assert service('install', '--config', str(config), '--service-user', 'operon-test', check=False).returncode != 0
    assert conflict.read_text() == '# unrelated service\n'
    conflict.unlink()  # only the fixture's own conflict file
    install('--respawn-delay-secs', '1', '--respawn-max', '10', '--respawn-period-secs', '30')
    installed = True
    assert Path('/etc/runlevels/default/operond').is_symlink()
    assert not daemon_pids(), 'install must enable boot but not start'
    assert not status()['running']
    original = config.read_bytes()
    config.write_text('invalid: [ yaml')
    assert service('start', check=False).returncode != 0
    assert not daemon_pids()
    config.write_bytes(original)
    denied = run('su', 'operon-test', '-s', '/bin/sh', '-c',
                 shlex.join([str(bins / 'operond'), 'service', '--backend', 'openrc', 'stop']), check=False)
    assert denied.returncode != 0 and b'requires root' in denied.stderr
    service('start', backend='auto')
    assert status()['healthy']
    pids = daemon_pids()
    assert len(pids) == 1
    assert Path(f'/proc/{pids[0]}').stat().st_uid == 1000
    executed = json.loads(cli('--json', 'exec', 'run', 'local', '--', 'id -u > identity.txt').stdout)
    assert executed['status'] == 'succeeded', executed
    assert cli('fs', 'read', 'local:/identity.txt').stdout.strip() == b'1000'
    cli('fs', 'write', 'local:/persist.txt', '--content', 'openrc-persist')
    os.kill(pids[0], signal.SIGKILL)
    wait(lambda: bool(daemon_pids()) and daemon_pids() != pids, 'supervise-daemon crash respawn')
    wait(lambda: status()['healthy'], 'respawn readiness')
    assert Path(f'/proc/{daemon_pids()[0]}').stat().st_uid == 1000
    detached = json.loads(cli('--json', 'exec', 'run', 'local', '--detach', '--timeout-secs', '300', '--',
                              'sleep 300 & echo $! > grandchild.pid; echo $$ > shell.pid; wait').stdout)
    assert detached['status'] in ['running', 'queued'], detached
    wait(lambda: (workspace / 'grandchild.pid').exists() and (workspace / 'shell.pid').exists(), 'owned exec children')
    children = [int((workspace / name).read_text()) for name in ['grandchild.pid', 'shell.pid']]
    service('stop')
    wait(lambda: not daemon_pids(), 'graceful daemon shutdown')
    for child in children:
        path = Path(f'/proc/{child}/stat')
        assert not path.exists() or path.read_text().rsplit(')', 1)[1].split()[0] == 'Z', 'OpenRC stop orphaned exec child'
    assert not status()['running']
    # The runner now reboots this same container through Alpine's real PID 1.
    # Retain registration and boot-runlevel symlink for the second stage.
    installed = False
    print('PASS: initial lifecycle; ready for real boot-enabled reboot')
except Exception:
    print('OpenRC diagnostic:', service('status', '--json', check=False).stdout.decode(errors='replace'))
    for path in [Path('/var/log/operon/stderr.log'), Path('/var/log/operon/stdout.log')]:
        if path.exists():
            print(path, path.read_text(errors='replace')[-12000:])
    raise
finally:
    if installed:
        service('uninstall', check=False)
