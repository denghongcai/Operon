# Embedded connectivity release Security review

Status: In progress; not a release-completion claim.

Latest source checkpoint `dab0e8ecbc8ddf330603b07754dbd84e5f27a180`
introduces two additional open CodeQL findings: #14 and #15,
`rust/access-invalid-pointer`, on the `#[napi]` NativeClient and NativeCall
class declarations. The reported dereferences originate in generated binding
code; source locations alone do not establish a false positive. Inspect the
expanded wrapper and object lifetime/null/type validation before disposition.
The current API read returns three open code-scanning alerts (#13–15), zero
Dependabot and zero secret-scanning alerts. No findings were dismissed.

Local wrapper review: pinned napi-derive-backend emits `napi_unwrap` status
checks, `validate_type_tag` (active because this crate enables napi8), then
`register_native_borrow_with_value` before FromNapiRef/FromNapiMutRef casts.
The pinned napi 3.14.2 borrow guard rejects null pointers and tracks aliases;
the rooted conversion scope keeps the source JavaScript object referenced.
Generated method receivers separately use `CallbackInfo::unwrap_raw` and
borrow guards. The actual Alpine Node fixture now verifies both classes reject
plain objects, prototype-spoofed objects and a wrapped object of the wrong class
before rerunning all capability checks. Those checks passed locally. This is
not a blanket disposition: asynchronous reference lifetime review, exact-source
online regressions and final alert review remain required.

Async generation uses `NativeBorrowScope::new_async`, roots receiver values,
and transfers the scope into the `execute_tokio_future_with_finalize_callback`
finalizer, releasing it after the future settles on the Node owner thread.
Actual Alpine acceptance with `--expose-gc` drops the last call object reference
while receive/close promises are pending, forces collection and verifies both
cancellation and joined cleanup. This passed; native hosted gates now run the
same regression. Final exact-source online evidence is still outstanding.

On 2026-10-11, a paginated GitHub API read of `denghongcai/Operon` returned six
open CodeQL alerts, no open Dependabot alerts and no open secret-scanning
alerts. All three APIs were accessible. Re-query against the final release
source and lockfile before tagging; these counts are a checkpoint, not proof
that future dependency changes are clean.

| Alerts | Finding | Disposition and required evidence |
| --- | --- | --- |
| CodeQL #1, #2, #7, #9 | `actions/missing-workflow-permissions` in CI | Add workflow-level `contents: read`; no CI job requires write privileges. Require fresh online CodeQL results. |
| CodeQL #12 | `rust/access-invalid-pointer` in Windows WinFsp creation | Check success output with `NonNull` before accessing `UserContext`; reject successful-but-null output and preserve failing NTSTATUS. Native Windows unit/compile/live-mount checks and rescanning remain required. |
| CodeQL #13 | `rust/cleartext-logging` in config explain | The source is policy `allowed_secrets`, a list of permitted **names**, not loaded secret values. Preserve intentional human/JSON policy diagnostics. Compiled CLI regression writes real token/secret files, checks names remain visible and both stdout/stderr exclude their sentinel values. Passed locally; require online regression and review final alert dataflow. |

For #13, `ExecPolicyExplain.allowed_secrets` is copied directly from
`PolicyConfig.exec.allowed_secrets`; secret file values are not inserted into
that model. No blanket dismissal, output removal or suppression was applied.
The regression is
`cli_static_integration::config_explain_prints_allowed_secret_names_but_never_values`.
The Windows output-pointer regressions are in `creation_result_tests`.

Checkpoint `283602d76334a92b17e95dda9c6382222b9c98f0` passed CodeQL
`38121899141`; a fresh API read returned only #13 open. All five remediated
findings are no longer open. Windows platform smoke in CI `38121899226`
passed the mount-adapter test step, including the checked-creation regressions.
This checkpoint's overall CI failed because the legacy Docker builder still
used Rust 1.85; it is not final release evidence. Upgrade that builder and
require a new exact-commit CI run. Native live mount and final-source Security
review remain release requirements.

Never print secret-scanning credentials in evidence or logs. A green CodeQL
workflow alone does not prove an alert was resolved. Before publication,
inspect the current open alert list, affected locations, analysis SHA and
justified not-affected findings, and record any permission gap explicitly.

Latest API checkpoint on 2026-10-11: CodeQL has one open finding (#13,
`rust/cleartext-logging`, severity high); Dependabot and secret scanning each
have zero open findings. These are current repository alert counts, not
security acceptance of the uncommitted native connectivity changes. The
workspace all-target tests, including the compiled CLI secret-value regression,
passed locally after those changes. Re-scan the final committed source and
lockfile and review #13 again before publication; do not represent the
repository as having zero open Security findings.
