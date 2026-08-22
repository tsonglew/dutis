# Policy and mutation audit

Dutis v2.9 evaluates every mutation against a local policy and creates a
persistent audit record before any `duti -s` command can run. This applies to
interactive changes, `set`, declarative `apply`, `rollback`, and MCP writes.

## Policy location

The default policy path is:

```text
~/Library/Application Support/dutis/policy.toml
```

Set `DUTIS_POLICY_FILE` to use another file. If the file does not exist, Dutis
uses a built-in policy with `approval_mode = "explicit"` and no allowlists.
Inspect the effective policy without exposing its token hash:

```bash
dutis policy show
dutis policy show --json
```

## Policy schema

Start from [`dutis.policy.example.toml`](../dutis.policy.example.toml):

```toml
version = 1
approval_mode = "explicit"
allowed_extensions = ["md", "txt"]
allowed_kinds = ["extension", "uti", "mime", "url_scheme"]
allowed_applications = ["com.microsoft.VSCode", "com.apple.TextEdit"]

[protected_associations]
pdf = "com.apple.Preview"

[[protected_handlers]]
kind = "uti"
identifier = "public.plain-text"
role = "viewer"
application = "com.apple.TextEdit"
```

- `allowed_extensions` limits changed extensions. Omit it to allow any
  extension; use an empty list to deny all extensions. It does not constrain
  other association kinds.
- `allowed_kinds` limits changes by normalized association kind. Omit it to
  allow all four kinds; use an empty list to deny all kinds.
- `allowed_applications` limits target bundle identifiers with the same
  omitted-versus-empty behavior.
- `protected_associations` permits an extension only when the target is the
  configured bundle identifier. This allows restoration to the protected value
  while denying changes away from it.
- `protected_handlers` applies the same protection to a typed kind, identifier,
  and role tuple. The role defaults to `all`; URL schemes accept only `all`.
- Unknown fields, invalid identifiers or roles, duplicate normalized targets,
  and unknown versions fail closed.

Check a configuration against the current system state and policy without
changing anything:

```bash
dutis policy check dutis.toml --json
```

## Approval modes

`approval_mode` accepts:

- `explicit`: require the existing interactive confirmation or CLI `--yes`.
- `token`: require a token whose SHA-256 digest matches
  `approval_token_sha256`.
- `deny`: disable every mutation while keeping inspection available.

Generate a token digest locally and place only the digest in the policy:

```bash
printf %s 'your-random-secret' | shasum -a 256
```

For CLI writes, provide the secret through `DUTIS_APPROVAL_TOKEN`. MCP writes
use the `approval_token` field already required by the write tool; the same
secret can satisfy both the MCP server gate and a token policy. Never commit or
log the plaintext token.

## Request identity

Use `--requester` for non-interactive CLI writes:

```bash
dutis apply dutis.toml \
  --plan-digest <reviewed-digest> \
  --requester codex \
  --yes
```

If omitted, CLI and interactive mode use `DUTIS_REQUESTER`, then the local
`USER`, then `local-user`. MCP writes require a non-empty `requester` argument.

## Persistent audit records

Audit records are owner-only JSON files stored in:

```text
~/Library/Application Support/dutis/audit/<audit-id>.json
```

Use `DUTIS_STATE_DIR` to relocate both snapshots and audit storage. Inspect
records newest first:

```bash
dutis audit
dutis audit --json
```

Every record includes the requester, channel (`cli`, `interactive`, `mcp`, or
`watcher`), operation, policy and plan
digests, and full reviewed plan. Completed mutation records also include the
safety snapshot ID, per-entry result, and verification summary. Dutis atomically
writes a `pending` record before a mutation. If policy denies the request, it
writes a `denied` record and never invokes the system mutation. If audit storage
cannot be prepared, the mutation is refused.

Configured [event sinks](event-sinks.md) receive the persisted mutation
lifecycle as `mutation.pending`, `mutation.denied`, `mutation.failed`, and
`mutation.completed` events. Event delivery is downstream of the safety
boundary: a sink failure is reported as a warning and cannot turn an audited
successful mutation into a failure or authorize a denied mutation.
