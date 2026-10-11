# Embedded connectivity release Security review

Status: In progress; not a release-completion claim.

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

Never print secret-scanning credentials in evidence or logs. A green CodeQL
workflow alone does not prove an alert was resolved. Before publication,
inspect the current open alert list, affected locations, analysis SHA and
justified not-affected findings, and record any permission gap explicitly.
