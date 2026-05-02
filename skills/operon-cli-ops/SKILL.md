---
name: operon-cli-ops
description: Use when an agent needs to inspect Operon nodes, capabilities, config, audit records, traces, or execution graphs through the CLI.
---

# Operon CLI Ops

Use this skill for operational inspection and scriptable CLI workflows. Prefer JSON output for automation:

```bash
operon --config <path> --json config explain
operon --config <path> --json node list
operon --config <path> --json capability list <node>
```

For exact flags, run the relevant help command first:

```bash
operon --help
operon config explain --help
operon node --help
operon capability --help
operon audit show --help
operon trace --help
operon run --help
```

Common scenarios:

- Need to know which config is active: `operon config explain`.
- Need to see reachable configured nodes: `operon node list`, then `operon node ping <node>`.
- Need LAN discovery: `operon node discover --provider lan`.
- Need to know what a node allows: `operon capability list <node>`.
- Need evidence for a change: `operon audit show <node>` with filters such as capability, action, allowed, resource, or limit.
- Need execution graph evidence: run a graph with `operon run <workflow.yaml> --trace-output <trace.json>`, then inspect with `operon trace show <trace.json>`.

For scripts, validate that `--json` produces one JSON document before piping it into downstream tools. Use `--quiet` only when the command result is represented by exit status or a side effect.
