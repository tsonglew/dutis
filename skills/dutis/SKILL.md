---
name: dutis
description: Inspect, plan, safely change, or roll back macOS default file applications with Dutis through its CLI or MCP tools.
---

# Dutis

Use Dutis for macOS filename-extension associations. Preserve its
`inspect -> plan -> policy check -> approval -> apply -> verify` boundary.

## Workflow

1. Start read-only. Check readiness and the effective policy, then inspect the
   requested extensions and installed handlers.
2. If the user asks for a general setup or is unsure which application to use,
   inspect `dutis_profiles` / `dutis_profile`, then use `dutis_recommend` (or
   the equivalent CLI commands). Present candidate evidence and treat the
   recommendation strictly as a proposal, never as approval.
3. Express multi-extension changes as versioned TOML. Build a fresh plan and
   run the policy check. Treat unresolved selectors or policy violations as a
   stop condition.
4. Show the user the exact changed extensions, target bundle IDs, plan digest,
   and any policy constraints. Run dry-run before requesting approval.
5. Ask for explicit approval immediately before a write. Never infer approval
   from an earlier inspection request.
6. Apply only the reviewed digest, include a meaningful requester identity,
   and report the resulting safety snapshot and audit IDs.
7. Verify with `dutis get`, then use `dutis audit` when the user needs the full
   local record.

For monitoring requests, use `dutis_drift` or `dutis watch <config> --once`
first. Explain whether the report is `in_sync`, `drift_detected`, or
`unresolved`. Do not enable `--remediate` or install a remediating LaunchAgent
without explicit user authorization for continuous system changes. A
notification-only LaunchAgent is the safe default.

For CLI workflows, prefer `dutis plan`, `dutis policy check`, and
`dutis apply --dry-run` before the confirmed `dutis apply` command with
`--yes`, `--plan-digest`, and `--requester`. For rollback, preview with
`dutis rollback <id> --dry-run` before the confirmed command.

For MCP workflows, use `dutis_diff` and `dutis_policy_check` before
`dutis_apply`, or `dutis_rollback_plan` before `dutis_rollback`. Mutation tools
must be advertised by the server and require a fresh digest, an approval token,
and `requester`. Never invent, persist, echo, or request disclosure of a secret
token; use only a token supplied through the authorized workflow.

Do not pass recommendation output directly to a write tool. Rebuild the plan
from the reviewed proposed TOML so current state and policy are checked again.

Do not bypass Dutis with raw `duti` mutation commands. Do not weaken or rewrite
the local policy unless the user explicitly asks to change that policy.
