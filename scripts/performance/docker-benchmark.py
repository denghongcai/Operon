#!/usr/bin/env python3
"""Isolated opt-in baseline/candidate FUSE benchmarks and runtime contracts.

Requires Docker, /dev/fuse, built Linux binaries and the built JS SDK.
Leaves evidence in --output; removes only containers/network created by this run.
"""
import argparse
import hashlib
import json
from pathlib import Path
import secrets
import shutil
import subprocess
import struct
import tempfile
import time

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--bin-dir',type=Path,default=Path('target/release'))
parser.add_argument('--baseline-bin-dir',type=Path)
parser.add_argument('--output',type=Path)
parser.add_argument('--operations',type=int,default=500)
parser.add_argument('--mib',type=int,default=64)
parser.add_argument('--repeats',type=int,default=2)
parser.add_argument('--client-image',default='node:22-bookworm')
parser.add_argument('--daemon-image',default='operon-node-a:latest')
args=parser.parse_args()
assert args.operations>0 and args.mib>0 and args.repeats>0
repo=Path(__file__).resolve().parents[2]
output=args.output.resolve() if args.output else Path(tempfile.mkdtemp(prefix='operon-phases-'))
output.mkdir(parents=True,exist_ok=True)
assert not any(output.iterdir()), 'Output directory must be empty to preserve prior evidence'
subprocess.run(['cc','-shared','-fPIC','-O2',str(repo/'scripts/performance/sync-counter.c'),'-ldl','-o',str(output/'sync-counter.so')],check=True)
prefix='operon-phases-'+secrets.token_hex(4)
network=prefix+'-net'; containers=[]

def docker(*arguments,check=True):
    result=subprocess.run(['docker',*map(str,arguments)],capture_output=True,text=True)
    if check and result.returncode: raise RuntimeError(result.stderr or result.stdout)
    return (result.stdout + (result.stderr if arguments and arguments[0]=='logs' else '')).strip()

def run(label,binaries,correctness):
    case=output/label; case.mkdir(); (case/'workspace').mkdir(); (case/'bin').mkdir()
    for name in ['operon','operond']: shutil.copy2(binaries/name,case/'bin'/name)
    shutil.copy2(output/'sync-counter.so',case/'bin'/'sync-counter.so')
    token=secrets.token_hex(32); (case/'token').write_text(token); (case/'token').chmod(0o600)
    config='''version: 1
daemon:
  node_id: local
  grpc_listen: 0.0.0.0:7789
  workspace: /workspace
  advertise_lan: false
  store: /review/store.jsonl
  auth:
    token_file: /review/token
client:
  nodes:
    local:
      endpoint: grpc://daemon:7789
      auth:
        token_file: /review/token
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
'''
    (case/'config.yaml').write_text(config)
    daemon=prefix+'-'+label+'-daemon'; client=prefix+'-'+label+'-client'
    docker('run','-d','--name',daemon,'--network',network,'--network-alias','daemon',
           '-e','LD_PRELOAD=/review/bin/sync-counter.so','-e','OPERON_SYNC_COUNTER=/review/sync-count.bin',
           '--user','root','--entrypoint','/review/bin/operond','-v',f'{case}:/review',
           '-v',f'{case}/workspace:/workspace',args.daemon_image,'start','--config','/review/config.yaml')
    containers.append(daemon)
    docker('run','-d','--name',client,'--network',network,'--device','/dev/fuse',
           '--cap-add','SYS_ADMIN','-v',f'{case}:/review','-v',f'{repo}:/repo:ro',
           '-v',f'{case}/bin/operon:/usr/local/bin/operon:ro',args.client_image,'sleep','infinity')
    containers.append(client)
    docker('exec',client,'sh','-c','command -v fusermount3 >/dev/null || (apt-get update -qq && apt-get install -y -qq fuse3)')
    for _ in range(200):
        try:
            docker('exec',client,'operon','--config','/review/config.yaml','node','ping','local')
            break
        except RuntimeError: time.sleep(.05)
    else: raise RuntimeError(docker('logs',daemon,check=False))
    if correctness:
        evidence=docker('exec',client,'node','/repo/scripts/performance/runtime-correctness.mjs')
        (case/'correctness.txt').write_text(evidence+'\n'); print(evidence,flush=True)
        # Also exercise real Rust clients with an 8MiB stream and verify bytes.
        source=case/'rust-source.bin'; source.write_bytes(bytes(range(251))*33422)
        docker('exec',client,'operon','--config','/review/config.yaml','fs','write','local:/rust.bin','--file','/review/rust-source.bin')
        docker('exec',client,'operon','--config','/review/config.yaml','fs','read','local:/rust.bin','--output','/review/rust-read.bin')
        docker('cp',f'{client}:/review/rust-read.bin',case/'rust-read-export.bin')
        assert (case/'rust-read-export.bin').read_bytes()==source.read_bytes()
    print(f'Running {label}',flush=True)
    cpu_before=docker('exec',daemon,'cat','/sys/fs/cgroup/cpu.stat',check=False)
    sync_before=json.loads(docker('exec',client,'python3','-c',
        'import struct,json;print(json.dumps(struct.unpack("=QQQ",open("/review/sync-count.bin","rb").read())))'))
    result=docker('exec',client,'python3','/repo/scripts/performance/fuse-workload.py',
                 '--label',label,'--operations',args.operations,'--mib',args.mib)
    (case/'benchmark-stdout.jsonl').write_text(result+'\n')
    bad=[]; counts={}
    exec_log_bytes=0
    # Docker may remap root ownership; export a caller-owned copy without
    # weakening the daemon's private store permissions.
    docker('cp',f'{client}:/review/store.jsonl',case/'store-export.jsonl')
    for index,line in enumerate((case/'store-export.jsonl').read_text().splitlines(),1):
        if not line.strip(): continue
        try: record=json.loads(line)
        except json.JSONDecodeError: bad.append(index); continue
        kind=record.get('kind','unknown'); counts[kind]=counts.get(kind,0)+1
        if kind=='exec_log': exec_log_bytes+=len(record['log']['data'])
    if correctness: assert not bad, f'Candidate produced invalid JSONL lines: {bad}'
    if correctness: assert exec_log_bytes==4194304+len('restart-marker'), 'Exec persisted byte count is incorrect'
    docker('cp',f'{client}:/review/sync-count.bin',case/'sync-count-export.bin')
    sync_calls,sync_nanoseconds,sync_failures=struct.unpack('=QQQ',(case/'sync-count-export.bin').read_bytes())
    assert sync_calls>0 and sync_failures==0
    resources=dict(cpu_before=cpu_before,cpu_after=docker('exec',daemon,'cat','/sys/fs/cgroup/cpu.stat',check=False),
                   memory_peak_bytes=docker('exec',daemon,'cat','/sys/fs/cgroup/memory.peak',check=False),
                   memory_current_bytes=docker('exec',daemon,'cat','/sys/fs/cgroup/memory.current',check=False))
    docker('restart',daemon)
    restored=False
    for _ in range(200):
        if docker('inspect','--format','{{.State.Status}}',daemon)=='exited': break
        try:
            docker('exec',client,'operon','--config','/review/config.yaml','node','ping','local')
            restored=True; break
        except RuntimeError: time.sleep(.05)
    if correctness:
        assert restored, 'Candidate failed real daemon restart'
        evidence=docker('exec','-e','RESTART_CHECK=1',client,'node','/repo/scripts/performance/runtime-correctness.mjs')
        with open(case/'correctness.txt','a') as file: file.write(evidence+'\n')
        print(evidence,flush=True)
    summary=dict(label=label,bad_jsonl_lines=bad,record_counts=counts,restart_success=restored,
                 exec_log_bytes=exec_log_bytes,sync_calls=sync_calls,sync_nanoseconds=sync_nanoseconds,resources=resources,
                 benchmark_sync_calls=sync_calls-sync_before[0],benchmark_sync_nanoseconds=sync_nanoseconds-sync_before[1],
                 binary_sha256={name:hashlib.sha256((case/'bin'/name).read_bytes()).hexdigest() for name in ['operon','operond']})
    (case/'summary.json').write_text(json.dumps(summary,indent=2))
    (case/'daemon.log').write_text(docker('logs',daemon,check=False))
    docker('stop','--time','2',client,daemon,check=False)
    docker('rm',client,daemon); containers.remove(client); containers.remove(daemon)
    return summary

try:
    docker('network','create',network)
    metadata=dict(head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),
                  dirty=subprocess.check_output(['git','status','--short'],cwd=repo,text=True),
                  docker=docker('version','--format','{{.Server.Version}}'),operations=args.operations,
                  mib=args.mib,repeats=args.repeats,client_image=args.client_image,daemon_image=args.daemon_image)
    (output/'environment.json').write_text(json.dumps(metadata,indent=2))
    summaries=[]
    for repetition in range(args.repeats):
        variants=[('candidate',args.bin_dir.resolve(),True)]
        if args.baseline_bin_dir: variants.insert(0,('baseline',args.baseline_bin_dir.resolve(),False))
        if repetition%2: variants.reverse()
        for label,binaries,correctness in variants: summaries.append(run(f'{label}-{repetition}',binaries,correctness))
    (output/'summary.json').write_text(json.dumps(summaries,indent=2))
finally:
    for container in reversed(containers):
        docker('stop','--time','2',container,check=False); docker('rm',container,check=False)
    docker('network','rm',network,check=False)
    print(f'Evidence retained: {output}',flush=True)
