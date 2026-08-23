---
name: dutis
description: Inspect, plan, safely change, or roll back macOS default handlers for extensions, UTIs, MIME types, and URL schemes with Dutis through its CLI or MCP tools.
---

# Dutis

Use Dutis for macOS Launch Services associations. Preserve its
`inspect -> plan -> policy check -> approval -> apply -> verify` boundary.

## Workflow

1. Start read-only. Check readiness and the effective policy, then inspect the
   requested association targets and installed handlers. Use `handler get` or
   `dutis_handler_get` for UTIs, MIME types, URL schemes, and role-specific
   defaults. Use `handler query` or `dutis_handler_query` to discover apps that
   explicitly register the requested kind and compatible role. Treat imported
   or exported UTI definitions as type evidence, not handler capability.
   When roles may differ, use `handler defaults` or
   `dutis_handler_defaults` to inspect the native Launch Services role matrix
   before planning a change.
   If an event command reports a delivery failure, inspect `events health`
   (`dutis_event_health` over MCP), then inspect `events pending` only when
   event-level details are necessary. Use `events replay` only after confirming
   the configured sink is healthy.
   Preview `events archive` and `events purge` before using `--yes`; purge is
   permanent and applies only to events already moved into dead-letter storage.
2. If the user asks for a general setup or is unsure which application to use,
   inspect `dutis_profiles` / `dutis_profile`, then use `dutis_recommend` (or
   the equivalent CLI commands). Present candidate evidence and treat the
   recommendation strictly as a proposal, never as approval.
   Treat profiles returned by Dutis as the effective catalog: local
   `profiles.toml` overlays may extend built-ins or add custom typed profiles.
   Never accept caller-supplied overlay contents through MCP.
   Treat candidate `policy_eligible`, `policy_reasons`, and `source` fields as
   required evidence; do not substitute caller-provided preferences for the
   effective local policy. For UTI, MIME, and URL-scheme recommendations,
   require `declares_target = true`; typed fleet preferences apply to every
   profile recommendation but never authorize a mutation.
3. Express multi-target changes as version 2 TOML. Build a fresh plan and
   run the policy check. Treat unresolved selectors or policy violations as a
   stop condition.
4. Show the user the exact changed kinds, identifiers, roles, target bundle IDs, plan digest,
   and any policy constraints. Run dry-run before requesting approval.
5. Ask for explicit approval immediately before a write. Never infer approval
   from an earlier inspection request.
6. Apply only the reviewed digest, include a meaningful requester identity,
   and report the resulting safety snapshot and audit IDs.
7. Verify with `dutis get`, then use `dutis audit` when the user needs the full
   local record.

When the user requests integration with another local tool, prefer the global
`--event-log` or `--event-command` sink. Treat an event command as executable
code: require a user-chosen trusted absolute path, and never generate or enable
one implicitly. Events may contain application paths, bundle IDs, requester
identity, and complete plans, but never approval tokens.

For HTTPS delivery, prefer the packaged `dutis-event-http` external command.
Keep `DUTIS_HTTP_ENDPOINT` and `DUTIS_HTTP_BEARER_TOKEN` out of Dutis TOML,
plans, prompts, and audit records. Run `dutis-event-http --check --json` before
enabling it, and use a Keychain-backed wrapper for a LaunchAgent rather than
persisting transport credentials in its plist.

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
