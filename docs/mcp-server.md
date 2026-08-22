# MCP server

Dutis v2.8 exposes its inspected, planned, snapshotted, and verified association
workflow as a local Model Context Protocol server. The server uses JSON-RPC 2.0
over stdio, so protocol messages are written only to stdout. Stable JSON audit
events are written to stderr.

## Start in read-only mode

Read-only is the default:

```bash
dutis mcp
```

A typical MCP client configuration is:

```json
{
  "mcpServers": {
    "dutis": {
      "command": "dutis",
      "args": ["mcp"]
    }
  }
}
```

The server advertises these read tools:

- `dutis_list`: discover installed applications and declared extensions.
- `dutis_query`: find installed handlers for one extension.
- `dutis_get`: inspect the current default handler.
- `dutis_diff`: parse inline versioned TOML and return a deterministic plan.
- `dutis_history`: list local safety snapshots.
- `dutis_rollback_plan`: preview a snapshot rollback and return its digest.

`dutis_diff` accepts `config_toml` rather than a filesystem path. This keeps the
MCP surface independent from unrestricted file access and makes the exact input
part of the reviewed tool call.

## Enable writes explicitly

Mutation tools are absent from `tools/list` unless the server starts with write
capabilities enabled. Use a secret of at least 16 characters:

```bash
DUTIS_MCP_APPROVAL_TOKEN='replace-with-a-random-secret' dutis mcp --allow-writes
```

For an MCP client, place the token in the server process environment:

```json
{
  "mcpServers": {
    "dutis": {
      "command": "dutis",
      "args": ["mcp", "--allow-writes"],
      "env": {
        "DUTIS_MCP_APPROVAL_TOKEN": "replace-with-a-random-secret"
      }
    }
  }
}
```

Write mode adds:

- `dutis_apply`: rebuild, apply, snapshot, and verify an inline TOML policy.
- `dutis_rollback`: rebuild, apply, snapshot, and verify a rollback.

Every write call must include both the approval token and the digest returned by
a fresh `dutis_diff` or `dutis_rollback_plan` call. Dutis rebuilds the plan from
current system state. A changed digest, unresolved selector, missing token, or
disabled write mode rejects the request before invoking `duti`.

Do not commit approval tokens to a repository or pass them as command-line
arguments. Rotate a token if it has been disclosed.

## Protocol and audit contract

The server supports MCP protocol versions `2024-11-05`, `2025-03-26`, and
`2025-06-18`. Tool results contain a text representation plus
`structuredContent` with `api_version: "1"`. Tool failures set `isError: true`
and return a stable error `kind` and message.

Each tool call emits one JSON audit event to stderr:

```json
{
  "schema_version": 1,
  "timestamp": "2026-08-22T00:00:00Z",
  "request_id": 7,
  "tool": "dutis_apply",
  "access": "write",
  "outcome": "success"
}
```

Audit events never include tool arguments or approval tokens. Snapshot storage
continues to use `DUTIS_STATE_DIR` when set.
