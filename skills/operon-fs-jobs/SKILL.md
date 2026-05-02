---
name: operon-fs-jobs
description: Use when an agent needs to read, write, copy, remove, or mount remote files, or run and interact with jobs on Operon nodes.
---

# Operon FS And Jobs

Use this skill for filesystem and process execution workflows. Targets use `node:/path` form. Always inspect the exact command help before running a command:

```bash
operon fs --help
operon fs read --help
operon fs write --help
operon job run --help
operon job logs --help
operon job stdin --help
```

Filesystem command choice:

- Inspect metadata: `operon fs stat <node:/path>`.
- List a directory: `operon fs list <node:/path>`.
- Stream a remote file: `operon fs read <node:/path>`.
- Write inline content or a local file: `operon fs write <node:/path>`.
- Create a directory: `operon fs mkdir <node:/path>`.
- Copy: `operon fs copy <node:/from> <node:/to>`.
- Rename or move: `operon fs rename <node:/from> <node:/to>`.
- Truncate: `operon fs truncate <node:/path> --size <bytes>`.
- Remove: `operon fs rm <node:/path>`.
- Linux local mount: `operon mount <node:/path> --to <mountpoint>`.

Confirm before `write`, `copy` over an existing destination, `rename`, `truncate`, `rm`, or mounting over a non-empty directory. Then verify with `fs stat`, `fs list`, and `audit show`.

Job command choice:

- Run a command and wait: `operon job run <node> -- <command>`.
- Start and return immediately: `operon job run <node> --detach -- <command>`.
- Check status: `operon job status <node> <job_id>`.
- Read or follow stdout/stderr: `operon job logs <node> <job_id>`.
- Send stdin or close it: `operon job stdin <node> <job_id>`.
- Cancel: `operon job cancel <node> <job_id>`.

Treat job execution as policy-sensitive. Confirm cwd, timeout, secrets, and destructive shell commands. Check `audit show` after running jobs that mutate files, services, or external systems.
