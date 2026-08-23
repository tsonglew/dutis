# Native Launch Services reads

Dutis v2.16 reads role-specific default handlers directly from macOS Launch
Services. This closes an important gap in `duti`: a single `duti -d` response
cannot distinguish viewer, editor, and shell defaults.

## Inspect every role

Use `handler defaults` to query the native registration database without
requiring `duti`:

```bash
dutis handler defaults extension txt --json
dutis handler defaults uti public.plain-text --json
dutis handler defaults mime text/plain --json
dutis handler defaults url-scheme https --json
```

For extensions and MIME types, Dutis first resolves the identifier to its
preferred UTI. The response includes that `content_type` plus separate `all`,
`viewer`, `editor`, and `shell` results. A missing registration is represented
by `null`. URL schemes have one role-free `all` result.

Example:

```json
{
  "schema_version": 1,
  "kind": "extension",
  "identifier": "txt",
  "content_type": "public.plain-text",
  "defaults": [
    { "role": "all", "bundle_id": "com.apple.TextEdit" },
    { "role": "viewer", "bundle_id": "com.example.Viewer" },
    { "role": "editor", "bundle_id": "com.apple.TextEdit" },
    { "role": "shell", "bundle_id": null }
  ]
}
```

The MCP equivalent is the read-only `dutis_handler_defaults` tool with `kind`
and `identifier` arguments.

## Role-aware reads and verification

`dutis handler get ... --role viewer|editor|shell` now uses the native API by
default. Declarative plans, drift checks, snapshots, and post-write
verification use the same native read whenever a specific role is requested.
This prevents a role-insensitive default from being mistaken for successful
verification.

Role `all` keeps the existing `duti` read path for backward-compatible output,
including application name and path for filename extensions. Mutations still
use `duti`; the native integration is read-only.

## Backend selection

The default backend mode is `auto`:

- specific content roles use native Launch Services on macOS;
- role `all` and URL-scheme reads use `duti` for compatibility;
- non-macOS builds retain the `duti` path and report that native queries are
  unavailable when requested explicitly.

For diagnostics, `DUTIS_HANDLER_READ_BACKEND=native` forces native reads and
`DUTIS_HANDLER_READ_BACKEND=duti` forces the compatibility path. Invalid
values fail closed. This setting changes only reads and verification; it never
enables a write.

`handler defaults` always uses the native API so its role matrix cannot
silently fall back to a role-insensitive source.

## Compatibility and safety

The implementation uses the stable Core Foundation and Launch Services C ABI
available across supported macOS releases. Owned Core Foundation values are
released exactly once, null results become `null`, and all input identifiers
pass through the same normalization used by the rest of Dutis.

Native reads do not execute shell commands, alter Launch Services, or require
mutation approval. Existing plan, policy, audit, snapshot, and rollback
boundaries remain unchanged.
