# Event sinks

Dutis v2.13 can deliver versioned drift and mutation events to a private JSONL
file, a trusted local executable, or both. This gives agents and automation a
stable interface without scraping terminal output or watching audit folders.

## Configure a sink

The global flags can appear before or after any subcommand:

```bash
dutis --event-log ./events.jsonl watch dutis.toml --once
dutis apply dutis.toml --dry-run --event-log ./events.jsonl
dutis --event-command /absolute/path/to/handler watch dutis.toml --once
```

Equivalent environment variables are useful for MCP servers, scripts, and
LaunchAgents:

```bash
export DUTIS_EVENT_LOG="$HOME/Library/Application Support/dutis/events.jsonl"
export DUTIS_EVENT_COMMAND="/absolute/path/to/handler"
```

Relative paths are resolved against the current working directory at startup.
An event command must resolve to an existing executable file. Dutis never runs
it through a shell.

## Event schema

Each event is one compact JSON object:

```json
{
  "schema_version": 1,
  "id": "1787366400000000000-1234-0",
  "emitted_at": "2026-08-22T00:00:00Z",
  "event_type": "drift.checked",
  "source": "watcher",
  "payload": {}
}
```

Supported event types are:

| Event | Payload |
| --- | --- |
| `drift.checked` | The complete drift report, plan, and policy assessment |
| `mutation.pending` | The durable pending mutation audit record |
| `mutation.denied` | The durable denied audit record and policy violations |
| `mutation.failed` | A pre-mutation failure record, such as a snapshot failure |
| `mutation.completed` | The final audit record with result and verification |

The `source` field is `watcher` for CLI/LaunchAgent checks, `mcp` for
`dutis_drift`, and `governance` for mutation lifecycle events. Mutation
payloads retain their more specific `channel` field.

Mutation events are emitted only after Dutis attempts the corresponding durable
audit update. Event payloads can contain local paths, bundle identifiers,
requester identity, and complete plans. Approval tokens and token hashes are
never included.

## JSONL sink

`--event-log` appends one event per line. Dutis creates a missing parent
directory with owner-only permissions and keeps the event file at mode `0600`
on Unix. Existing parent-directory permissions are not changed.

Log rotation is managed externally. A safe rotation replaces or renames the
file between Dutis invocations; a long-running watcher opens the file for each
event, so it follows the new path on the next check.

## Command sink

For each event, Dutis starts the configured executable and:

- writes exactly one JSON event followed by a newline to stdin;
- sets `DUTIS_EVENT_ID` and `DUTIS_EVENT_TYPE` for routing;
- discards stdout so JSON CLI and MCP protocol streams remain valid;
- captures stderr and reports a concise warning when the command fails.

Dutis removes its approval-token variables from the child environment before
starting the handler. Other environment variables are inherited so the handler
can use its own separately scoped credentials.

The command runs synchronously. Keep handlers short and delegate slow delivery
to a queue or background service. Store webhook credentials in the handler's
own protected configuration rather than in Dutis arguments, policy, or event
payloads.

Example handler:

```sh
#!/bin/sh
exec /usr/bin/logger -t dutis-event
```

## Failure behavior

Event sinks are observability outputs, not part of authorization. A sink
failure prints a warning to stderr and does not:

- permit a policy-denied mutation;
- bypass the mandatory audit or safety snapshot;
- roll back or relabel an already completed mutation;
- stop a continuous watcher from performing later checks.

When both sinks are configured, Dutis attempts both even if one fails.
