# Security Hardening Notes

This note records the operator-facing security defaults that protect the v0.16.9
release line and later local validation.

## Listener authentication

Daemon listeners that bind a non-loopback address require bearer-token auth at
startup. Use `daemon.auth.token_file` or `daemon.auth.token_env` for LAN,
wildcard, and private-network binds. Loopback-only local development listeners
may omit auth.

Daemon startup validates `daemon.auth.token_file` as a private file before
reading it. On Unix, the token file must be a regular file without group or
other permission bits. Generated onboarding token files already use owner-only
permissions.

## Doctor diagnostics

`operon doctor` reports `security_diagnostics` in JSON and human-readable output.
The diagnostics cover:

- non-loopback daemon listeners without auth,
- daemon and client inline token references,
- daemon, client, config, and secrets private-file permission checks, and
- services whose permissions are default-deny because neither `check` nor
  `forward` is enabled.

## Service permissions

Service permissions are default-deny when omitted. Set `permissions.check` and
`permissions.forward` explicitly for every intended service action. Check-only
services cannot be forwarded; forward-only services cannot be checked.

## Validation

Run the focused security validation locally before publishing release artifacts:

```bash
scripts/verify-security-hardening.sh
```

The script is also wired into the consolidated `core` validation group through
`scripts/ci/run-validations.sh`.
