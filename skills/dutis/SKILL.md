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
2. Express multi-extension changes as versioned TOML. Build a fresh plan and
   run the policy check. Treat unresolved selectors or policy violations as a
   stop condition.
3. Show the user the exact changed extensions, target bundle IDs, plan digest,
   and any policy constraints. Run dry-run before requesting approval.
4. Ask for explicit approval immediately before a write. Never infer approval
   from an earlier inspection request.
5. Apply only the reviewed digest, include a meaningful requester identity,
   and report the resulting safety snapshot and audit IDs.
6. Verify with `dutis get`, then use `dutis audit` when the user needs the full
   local record.

For CLI workflows, prefer `dutis plan`, `dutis policy check`, and
`dutis apply --dry-run` before the confirmed `dutis apply` command with
`--yes`, `--plan-digest`, and `--requester`. For rollback, preview with
`dutis rollback <id> --dry-run` before the confirmed command.

For MCP workflows, use `dutis_diff` and `dutis_policy_check` before
`dutis_apply`, or `dutis_rollback_plan` before `dutis_rollback`. Mutation tools
must be advertised by the server and require a fresh digest, an approval token,
and `requester`. Never invent, persist, echo, or request disclosure of a secret
token; use only a token supplied through the authorized workflow.

Do not bypass Dutis with raw `duti` mutation commands. Do not weaken or rewrite
the local policy unless the user explicitly asks to change that policy.
