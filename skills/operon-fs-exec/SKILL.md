---
name: operon-fs-execs
description: Use when an agent needs to read, write, copy, remove, or mount remote files, or run and interact with execs on Operon nodes.
---

# Operon FS And Execs

Use this skill for filesystem and process execution workflows. Graph steps use
`exec.run` for non-interactive execution. Filesystem targets use `node:/path`
form. Always inspect the exact command help before running a command:

```bash
operon fs --help
operon fs read --help
operon fs write --help
operon exec run --help
operon exec session --help
operon exec logs --help
operon exec stdin --help
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

Exec command choice:

- Run a command and wait: `operon exec run <node> -- <command>`.
- Start and return immediately: `operon exec run <node> --detach -- <command>`.
- Open a PTY-backed interactive command when policy allows sessions:
  `operon exec session <node> -- <command>`.
  The CLI uses the attached terminal size by default and forwards Unix resize
  events during interactive sessions.
- Check status: `operon exec status <node> <exec_id>`.
- Read or follow stdout/stderr: `operon exec logs <node> <exec_id>`.
- Send stdin or close it: `operon exec stdin <node> <exec_id>`.
- Cancel: `operon exec cancel <node> <exec_id>`.

Use `exec.run` for non-interactive commands and graph steps. Use
`exec.session` only when the command needs terminal semantics such as shell
line editing, terminal control, or PTY-aware programs. Treat exec execution as
policy-sensitive. Confirm cwd, timeout, secrets, and destructive shell
commands. Check `audit show` after running execs that mutate files, services,
or external systems.
