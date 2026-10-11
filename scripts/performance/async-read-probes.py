#!/usr/bin/env python3
"""Run only inside a disposable benchmark client container after timed cases."""
import concurrent.futures
import argparse
import errno
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--mode', choices=['async', 'sync'], default='async')
    parser.add_argument('--trace', action='store_true')
    parser.add_argument('--pending', type=int, default=64)
    parser.add_argument('--concurrency', type=int, default=16)
    parser.add_argument('--budget-mib', type=int, default=32)
    parser.add_argument('--blackhole-unmount', action='store_true')
    parser.add_argument('--lazy-method', choices=['helper', 'kernel'], default='helper')
    args = parser.parse_args()
    root, mount = Path('/review'), Path('/mnt/remote')
    payload = (bytes(range(251)) * 5000)[:1024 * 1024]
    for name in ['probe-correct', 'probe-timeout', 'probe-unmount']:
        (root / 'workspace' / name).write_bytes(payload)
    results = {'read_limits': {'pending': args.pending, 'concurrency': args.concurrency,
                               'budget_mib': args.budget_mib}}
    launch_count = 0
    def checkpoint():
        (root / 'async-probes.json').write_text(json.dumps(results, indent=2))
    def tc(delay):
        subprocess.run(['tc', 'qdisc', 'replace', 'dev', 'eth0', 'root',
                        'netem', 'delay', f'{delay}ms'], check=True)
    def launch(timeout, config='/review/config.yaml'):
        nonlocal launch_count
        log = open(root / 'async-probes-mount.log', 'a')
        process = subprocess.Popen(['/review/bin/operon', '--config', config,
                                    '--rpc-timeout-secs', str(timeout), 'mount', 'local:/',
                                    '--to', str(mount), '--mount-workers', '4',
                                    '--read-concurrency', str(args.concurrency),
                                    '--read-budget-mib', str(args.budget_mib),
                                    '--read-pending', str(args.pending),
                                    '--read-mode', args.mode],
                                   stdout=log, stderr=log,
                                   env={**os.environ, **({'OPERON_MOUNT_TRACE': '1'} if args.trace else {})})
        log.close()
        deadline = time.monotonic() + 15
        while not os.path.ismount(mount):
            if process.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError((root / 'async-probes-mount.log').read_text())
            time.sleep(.02)
        launch_count += 1
        return process
    def shutdown(process, lazy=False):
        if lazy and os.path.ismount(mount):
            helper_start = time.monotonic()
            if args.lazy_method == 'helper':
                command = ['fusermount3', '-uz', str(mount)]
                if args.trace:
                    command = ['strace', '-ff', '-tt', '-T', '-o', str(root / 'unmount-helper.trace'), *command]
                # The test process itself owns FUSE descriptors. Closing their
                # inherited copies before exec invokes FUSE_FLUSH and can block
                # behind saturated synchronous read callbacks BEFORE the helper
                # even starts. Keep them through exec; close after actual detach.
                subprocess.run(command, check=True, pass_fds=tuple(descriptors))
            else:
                # Diagnostic control: no user-space helper canonicalization.
                # Only the disposable container's exact, verified mount is in scope.
                import ctypes
                library = ctypes.CDLL(None, use_errno=True)
                unmount = library.umount2
                unmount.argtypes = [ctypes.c_char_p, ctypes.c_int]
                unmount.restype = ctypes.c_int
                if unmount(os.fsencode(mount), 2 | 8) != 0:  # MNT_DETACH | UMOUNT_NOFOLLOW
                    code = ctypes.get_errno()
                    raise OSError(code, os.strerror(code), str(mount))
            results['external_lazy_helper_seconds'] = time.monotonic() - helper_start
            results['external_lazy_method'] = args.lazy_method
        if not lazy:
            process.send_signal(signal.SIGINT)
        exit_code = process.wait(timeout=8)
        assert exit_code == 0, f'mount exit={exit_code}: {(root / "async-probes-mount.log").read_text()}'
        assert not os.path.ismount(mount)
    process = None
    descriptors = []
    try:
        # Normal queue/RPC deadline with fresh, unread pages.
        tc(0)
        process = launch(1)
        descriptors = [os.open(mount / name, os.O_RDONLY)
                       for name in ['probe-correct', 'probe-timeout']]
        fd = descriptors[0]
        assert os.pread(fd, 8192, 37) == payload[37:37 + 8192]
        assert os.pread(fd, 4096, len(payload)) == b''
        assert os.pread(fd, 0, 0) == b''
        target = mount / 'probe-write'
        target.write_bytes(b'async read keeps write-through')
        renamed = mount / 'probe-renamed'
        target.rename(renamed)
        assert renamed.read_bytes() == b'async read keeps write-through'
        assert (root / 'workspace' / 'probe-renamed').read_bytes() == b'async read keeps write-through'
        with open(renamed, 'r+b') as file:
            file.truncate(5)
        assert renamed.read_bytes() == b'async'
        assert (root / 'workspace' / 'probe-renamed').read_bytes() == b'async'
        renamed.unlink()
        assert not (root / 'workspace' / 'probe-renamed').exists()
        results['offset_eof_zero_write_rename_truncate_delete'] = 'passed'
        tc(3000)
        def timed_read(index):
            start = time.monotonic()
            try:
                os.pread(descriptors[1], 4096, index * 128 * 1024)
            except OSError as error:
                assert error.errno in (errno.EIO, errno.ETIMEDOUT), error
                return time.monotonic() - start
            raise AssertionError('Delayed read unexpectedly succeeded')
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            durations = list(pool.map(timed_read, range(4)))
        assert max(durations) < 4, durations
        results['deadline_seconds'] = durations
        tc(0)
        time.sleep(3.2)  # Let discarded netem/TCP packets recover; outside timing.
        assert os.pread(descriptors[1], 4096, 0) == payload[:4096]
        results['read_after_timeout_releases_budget'] = 'passed'
        for fd in descriptors:
            os.close(fd)
        descriptors = []
        shutdown(process)
        process = None
        results['normal_unmount'] = 'passed'
        checkpoint()
        # Keepalive tears down stale connections, but is not a deadline for
        # channel reconnection/admission. Bound the complete read with the
        # configured RPC deadline and verify recovery without remount.
        loss_config = root / 'probe-loss-config.yaml'
        loss_config.write_text((root / 'config.yaml').read_text().replace(
            '      endpoint: grpc://daemon:7789\n',
            '      endpoint: grpc://daemon:7789\n'
            '      transport:\n'
            '        connect_timeout_secs: 1\n'
            '        keepalive_interval_secs: 1\n'
            '        keepalive_timeout_secs: 1\n'))
        (root / 'workspace' / 'probe-loss').write_bytes(payload)
        process = launch(1, str(loss_config))
        descriptors = [os.open(mount / 'probe-loss', os.O_RDONLY)]
        subprocess.run(['tc', 'qdisc', 'replace', 'dev', 'eth0', 'root',
                        'netem', 'loss', '100%'], check=True)
        reader = subprocess.Popen(['python3', '-c',
            'import os,sys,json,time; t=time.monotonic(); '
            '\ntry: os.pread(int(sys.argv[1]),4096,0); result={"unexpected_success":True}'
            '\nexcept OSError as e: result={"errno":e.errno,"seconds":time.monotonic()-t}'
            '\nprint(json.dumps(result),flush=True)', str(descriptors[0])],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            pass_fds=tuple(descriptors))
        try:
            output, errors = reader.communicate(timeout=8)
            assert reader.returncode == 0, errors
            failure = json.loads(output)
            assert failure.get('errno') in (errno.EIO, errno.ETIMEDOUT), failure
            results['blackhole_configured_deadline'] = failure
        finally:
            tc(0)
            if reader.poll() is None:
                reader.kill()
                reader.wait(timeout=3)
        time.sleep(3.2)
        assert os.pread(descriptors[0], 4096, 0) == payload[:4096]
        results['read_after_blackhole_recovery'] = 'passed'
        os.close(descriptors[0])
        descriptors = []
        shutdown(process)
        process = None
        checkpoint()
        # Explicit shutdown must also work with a user-held file descriptor.
        process = launch(0)
        descriptors = [os.open(mount / 'probe-correct', os.O_RDONLY)]
        shutdown(process)
        process = None
        results['normal_unmount_with_open_descriptor'] = 'passed'
        for fd in descriptors:
            try:
                os.close(fd)
            except OSError as error:
                if error.errno != errno.ENOTCONN:
                    raise
        descriptors = []
        checkpoint()
        helper = root / 'bin' / 'mount-session-lifecycle'
        if helper.is_file():
            for action in ['drop', 'unmount']:
                helper_log = open(root / f'session-{action}.log', 'w')
                owner = subprocess.Popen([str(helper), 'grpc://daemon:7789',
                                          str(root / 'token'), str(mount), args.mode,
                                          str(args.pending), str(args.concurrency), str(args.budget_mib)],
                                         stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                         stderr=helper_log, text=True)
                helper_log.close()
                try:
                    assert owner.stdout.readline().strip() == 'ready'
                    descriptors = [os.open(mount / 'probe-correct', os.O_RDONLY)]
                    tc(3000)
                    def owned_pending_read(index):
                        try:
                            os.pread(descriptors[0], 4096, index * 128 * 1024)
                        except OSError as error:
                            return error.errno
                        raise AssertionError('Owned session shutdown left a delayed read successful')
                    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
                        reads = [pool.submit(owned_pending_read, i) for i in range(4)]
                        time.sleep(.2)
                        owner.stdin.write(action + '\n')
                        owner.stdin.flush()
                        stdout, _ = owner.communicate(timeout=8)
                        results[f'session_{action}_inflight_errno'] = [read.result(timeout=3) for read in reads]
                    assert owner.returncode == 0 and stdout.strip() == 'joined', (stdout, owner.returncode)
                    assert not os.path.ismount(mount)
                    log = (root / f'session-{action}.log').read_text()
                    assert 'active_after_join=0' in log and 'cleanup failed' not in log, log
                    results[f'session_{action}_with_open_descriptor'] = 'passed'
                finally:
                    tc(0)
                    if owner.poll() is None:
                        owner.kill()
                        owner.wait(timeout=3)
                    for fd in descriptors:
                        try:
                            os.close(fd)
                        except OSError as error:
                            if error.errno != errno.ENOTCONN:
                                raise
                    descriptors = []
                checkpoint()
        # Nonfatal signal: the kernel must route FUSE_INTERRUPT for this read.
        # ctypes avoids Python's automatic EINTR retry, so EINTR is observable.
        (root / 'workspace' / 'probe-interrupt').write_bytes(payload)
        process = launch(0)
        child_program = r'''
import ctypes, errno, json, os, signal, sys
signal.signal(signal.SIGUSR1, lambda *_: None)
signal.siginterrupt(signal.SIGUSR1, True)
fd = os.open('/mnt/remote/probe-interrupt', os.O_RDONLY | os.O_DIRECT)
print('ready', flush=True)
assert sys.stdin.readline().strip() == 'read'
lib = ctypes.CDLL(None, use_errno=True)
read = getattr(lib, 'pread64', lib.pread)
read.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_longlong]
read.restype = ctypes.c_ssize_t
buffer = ctypes.c_void_p()
lib.posix_memalign.argtypes = [ctypes.POINTER(ctypes.c_void_p), ctypes.c_size_t, ctypes.c_size_t]
lib.posix_memalign.restype = ctypes.c_int
assert lib.posix_memalign(ctypes.byref(buffer), 4096, 4096) == 0
result = read(fd, buffer, 4096, 0)
code = ctypes.get_errno()
print(json.dumps({'bytes': result, 'errno': code}), flush=True)
os.close(fd)
lib.free.argtypes = [ctypes.c_void_p]
lib.free(buffer)
assert result == -1 and code == errno.EINTR, (result, code)
'''
        interrupt_reader = subprocess.Popen(['python3', '-c', child_program],
                                            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                            stderr=subprocess.PIPE, text=True)
        try:
            assert interrupt_reader.stdout.readline().strip() == 'ready'
            tc(3000)
            interrupt_reader.stdin.write('read\n')
            interrupt_reader.stdin.flush()
            time.sleep(.2)
            started = time.monotonic()
            interrupt_reader.send_signal(signal.SIGUSR1)
            stdout, stderr = interrupt_reader.communicate(timeout=3)
            assert interrupt_reader.returncode == 0, (stdout, stderr)
            result = json.loads(stdout)
            assert result == {'bytes': -1, 'errno': errno.EINTR}, result
            results['fuse_interrupt_seconds'] = time.monotonic() - started
            results['fuse_interrupt_errno'] = result['errno']
        finally:
            if interrupt_reader.poll() is None:
                interrupt_reader.kill()
                interrupt_reader.wait(timeout=3)
            tc(0)
        time.sleep(3.2)
        assert (mount / 'probe-interrupt').read_bytes() == payload
        results['read_after_interrupt_releases_budget'] = 'passed'
        shutdown(process)
        process = None
        checkpoint()
        # Disable deadlines: successful shutdown must cancel, not wait for timeout.
        process = launch(0)
        descriptors = [os.open(mount / 'probe-unmount', os.O_RDONLY)]
        if args.blackhole_unmount:
            subprocess.run(['tc', 'qdisc', 'replace', 'dev', 'eth0', 'root',
                            'netem', 'loss', '100%'], check=True)
        else:
            tc(3000)
        results['inflight_unmount_fault'] = 'loss_100_percent' if args.blackhole_unmount else 'delay_3000ms'
        def pending_read(index):
            started = time.monotonic()
            try:
                data = os.pread(descriptors[0], 4096, index * 128 * 1024)
            except OSError as error:
                return error.errno
            raise AssertionError(f'Unmounted delayed read unexpectedly succeeded: index={index}, bytes={len(data)}, elapsed={time.monotonic() - started:.6f}s')
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            pending = [pool.submit(pending_read, i) for i in range(4)]
            time.sleep(.2)
            start = time.monotonic()
            shutdown(process, lazy=True)
            process = None
            errors = [future.result(timeout=3) for future in pending]
            results['inflight_unmount_seconds'] = time.monotonic() - start
            results['inflight_unmount_errno'] = errors
        assert results['inflight_unmount_seconds'] < 3
        log = (root / 'async-probes-mount.log').read_text()
        assert log.count('active_after_join=0') == launch_count, log
        results['zero_active_after_initial_joins'] = 'passed'
        if args.mode == 'sync':
            results['synchronous_callbacks_joined'] = 'passed'
        checkpoint()
        # Non-lazy external unmount must also end the foreground process itself.
        tc(0)
        process = launch(0)
        subprocess.run(['fusermount3', '-u', str(mount)], check=True)
        assert process.wait(timeout=8) == 0
        process = None
        results['external_normal_unmount_without_signal'] = 'passed'
        checkpoint()
        # A new mount at the same path must never be unmounted by the old owner.
        tc(0)
        process = launch(0)
        subprocess.run(['fusermount3', '-uz', str(mount)], check=True)
        subprocess.run(['mount', '-t', 'tmpfs', 'operon-probe-replacement', str(mount)], check=True)
        try:
            sentinel = mount / 'replacement-owned-by-probe'
            sentinel.write_bytes(b'replacement must survive old session shutdown')
            assert process.wait(timeout=8) == 0
            process = None
            assert os.path.ismount(mount), 'Old session unmounted replacement'
            assert sentinel.read_bytes() == b'replacement must survive old session shutdown'
            results['replacement_mount_preserved'] = 'passed'
        finally:
            if os.path.ismount(mount):
                subprocess.run(['umount', str(mount)], check=True)
        assert (root / 'async-probes-mount.log').read_text().count('active_after_join=0') == launch_count
        results['zero_active_after_all_joins'] = 'passed'
        results['status'] = 'passed'
        checkpoint()
        print(json.dumps(results), flush=True)
    except Exception as error:
        results['status'] = 'failed'
        results['failure'] = repr(error)
        checkpoint()
        raise
    finally:
        tc(0)
        if os.path.ismount(mount):
            subprocess.run(['fusermount3', '-uz', str(mount)], check=True)
        if process is not None and process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        for fd in descriptors:
            try:
                os.close(fd)
            except OSError as error:
                if error.errno != errno.ENOTCONN:
                    raise


if __name__ == '__main__':
    main()
