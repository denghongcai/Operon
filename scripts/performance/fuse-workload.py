#!/usr/bin/env python3
"""Runs inside the disposable client created by docker-benchmark.py."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--label', required=True)
parser.add_argument('--operations', type=int, default=500)
parser.add_argument('--mib', type=int, default=64)
args = parser.parse_args()
root = Path('/review')
mountpoint = Path('/mnt/remote')
mountpoint.mkdir(parents=True, exist_ok=True)
rows = []

def network():
    line = next(x for x in Path('/proc/net/dev').read_text().splitlines() if 'eth0:' in x)
    fields = line.split(':')[1].split()
    return int(fields[0]), int(fields[8])

def audit():
    counts = {}
    path = root / 'store.jsonl'
    if path.exists():
        for line in path.read_text().splitlines():
            try: record = json.loads(line)
            except json.JSONDecodeError: continue
            if record.get('kind') == 'audit':
                action = record['event']['action']
                counts[action] = counts.get(action, 0) + 1
    return counts

def mount():
    log = open(root/'mount.log', 'a')
    process = subprocess.Popen(['operon', '--config', '/review/config.yaml', 'mount', 'local:/', '--to', str(mountpoint)], stdout=log, stderr=log)
    log.close()
    for _ in range(200):
        if os.path.ismount(mountpoint): return process
        if process.poll() is not None: raise RuntimeError((root/'mount.log').read_text())
        time.sleep(.05)
    process.terminate()
    raise RuntimeError('mount timed out')

def unmount(process):
    subprocess.run(['fusermount3', '-u', str(mountpoint)], check=True)
    process.terminate()
    process.wait(timeout=10)

def measure(name, function):
    rx, tx = network(); before = audit(); start = time.perf_counter()
    result = function()
    elapsed = time.perf_counter()-start
    end_rx, end_tx = network(); after = audit()
    row = dict(label=args.label, test=name, seconds=elapsed, rx_bytes=end_rx-rx,
               tx_bytes=end_tx-tx, audit_delta={k:v-before.get(k,0) for k,v in after.items() if v != before.get(k,0)}, result=result)
    # One operation is one complete iteration/file transfer, not an RPC or a
    # kernel-cache hit. Bytes and RPC counters remain independent measurements.
    if isinstance(result, dict) and result.get('returncode', 0) != 0:
        row['operations'] = 0
    elif isinstance(result, dict) and 'operations' in result:
        row['operations'] = result['operations']
    elif isinstance(result, list):
        row['operations'] = len(result)
    else:
        row['operations'] = 1
    row['ops_per_second'] = row['operations'] / elapsed
    rows.append(row)
    with open(root/'measurements.jsonl','a') as output: output.write(json.dumps(row)+'\n')
    print(json.dumps(row), flush=True)

def small(path, kind):
    path.mkdir(exist_ok=True)
    samples = []
    if kind == 'persistent': handle = open(path/'persistent','wb',buffering=0)
    for i in range(args.operations):
        start = time.perf_counter()
        if kind == 'persistent': handle.seek((i % 32)*4096); handle.write(b'x'*4096)
        elif kind == 'create':
            with open(path/f'create-{i}','wb'): pass
        elif kind == 'dd':
            subprocess.run(['dd','if=/dev/zero',f'of={path}/dd-{i}','bs=4096','count=1','status=none'],check=True)
        else:
            with open(path/f'file-{i}','wb',buffering=0) as output: output.write(b'x'*4096)
        samples.append((time.perf_counter()-start)*1000)
    if kind == 'persistent': handle.close()
    samples.sort()
    return dict(operations=args.operations, mean_ms=statistics.mean(samples),
                p50_ms=statistics.median(samples), p95_ms=samples[int(len(samples)*.95)], p99_ms=samples[int(len(samples)*.99)])

def read(path):
    digest = hashlib.sha256(); count = 0
    with open(path,'rb',buffering=0) as source:
        while chunk := source.read(1024*1024): digest.update(chunk); count += len(chunk)
    assert count == args.mib*1024*1024
    assert digest.hexdigest() == expected_digest
    return count

expected_digest = hashlib.sha256(b'Z'*(args.mib*1024*1024)).hexdigest()
for kind in ['persistent','file','create','dd']:
    measure('local/'+kind, lambda k=kind: small(Path('/dev/shm')/f'operon-{k}',k))
process = mount()
try:
    run = mountpoint/f'run-{time.time_ns()}'
    run.mkdir()
    for kind in ['persistent','file','create','dd']:
        measure('fuse/'+kind, lambda k=kind:small(run/k,k))
    def write_big():
        with open(mountpoint/'large.bin','wb',buffering=0) as output:
            for _ in range(args.mib): output.write(b'Z'*(1024*1024))
        return args.mib*1024*1024
    measure('fuse/write-sequential', write_big)
finally: unmount(process)
process = mount()
try:
    measure('fuse/read-client-cold',lambda:read(mountpoint/'large.bin'))
    measure('fuse/read-hot',lambda:read(mountpoint/'large.bin'))
    def parallel(paths):
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool: return list(pool.map(read,paths))
    measure('fuse/read-same-file-4',lambda:parallel([mountpoint/'large.bin']*4))
finally: unmount(process)
# Distinct remote files are populated locally by the orchestrator below.
subprocess.run(['operon','--config','/review/config.yaml','fs','copy','local:/large.bin','local:/distinct-0.bin'],check=True,stdout=subprocess.DEVNULL)
for i in range(1,4):
    subprocess.run(['operon','--config','/review/config.yaml','fs','copy','local:/large.bin',f'local:/distinct-{i}.bin'],check=True,stdout=subprocess.DEVNULL)
process=mount()
try: measure('fuse/read-distinct-files-4',lambda:parallel([mountpoint/f'distinct-{i}.bin' for i in range(4)]))
finally: unmount(process)
# Include binary streaming read and an exec output workload, preserving bytes.
def cli_read():
    result=subprocess.run(['operon','--config','/review/config.yaml','fs','read','local:/large.bin','--output','/review/cli-read.bin'],check=True,stdout=subprocess.DEVNULL)
    assert hashlib.sha256((root/'cli-read.bin').read_bytes()).hexdigest()==expected_digest
    return args.mib*1024*1024
measure('cli/read-stream',cli_read)

def cli_transfer(arguments, output=None):
    # wait4(Python child) can include the parent's pre-exec RSS high-water mark.
    # GNU time launches CLI from a small fresh process and reports CLI-only RSS.
    resource_file = root/'cli-transfer-rss.txt'
    process = subprocess.Popen(['/usr/bin/time', '-f', '%M', '-o', str(resource_file),
                                'operon', '--config', '/review/config.yaml', *arguments],
                               stdout=output if output is not None else subprocess.DEVNULL)
    _, status, _usage = os.wait4(process.pid, 0)
    process.returncode = os.waitstatus_to_exitcode(status)
    assert process.returncode == 0
    return dict(operations=1, bytes=args.mib*1024*1024, max_rss_kib=int(resource_file.read_text().strip()))

def cli_write():
    result = cli_transfer(['fs', 'write', 'local:/cli-upload.bin', '--file', '/review/workspace/large.bin'])
    assert hashlib.sha256((root/'workspace/cli-upload.bin').read_bytes()).hexdigest() == expected_digest
    return result

def cli_raw_read():
    with open(root/'cli-raw-read.bin', 'wb') as output:
        result = cli_transfer(['fs', 'read', 'local:/large.bin'], output)
    assert hashlib.sha256((root/'cli-raw-read.bin').read_bytes()).hexdigest() == expected_digest
    return result

measure('cli/write-stream', cli_write)
measure('cli/read-raw', cli_raw_read)
def exec_output():
    result=subprocess.run(['operon','--config','/review/config.yaml','exec','run','local','--cwd','/','--argv','--','/usr/bin/head','-c','4194304','/dev/zero'],capture_output=True)
    # Human exec output may also contain the execution summary; persisted logs
    # are separately inspected by the orchestrator for the exact byte count.
    if args.label.startswith('candidate') and result.returncode:
        raise RuntimeError(result.stderr.decode(errors='replace')[:1000])
    return dict(stdout_bytes=len(result.stdout),returncode=result.returncode,stderr=result.stderr.decode(errors='replace')[:1000])
measure('exec/output-4MiB',exec_output)
(root/'results.json').write_text(json.dumps(rows,indent=2))
