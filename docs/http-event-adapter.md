# HTTP event adapter

Dutis v2.15 includes `dutis-event-http`, an optional external event command
that forwards versioned Dutis events to an HTTPS endpoint. It does not change
the core event-sink trust boundary: delivery remains best effort and cannot
authorize, alter, or roll back a mutation.

## Quick start

Configure the adapter through its environment, validate it without making a
network request, then select the executable as the Dutis command sink:

```bash
export DUTIS_HTTP_ENDPOINT='https://events.example.com/hooks/dutis'
export DUTIS_HTTP_BEARER_TOKEN='replace-with-a-scoped-token'
export DUTIS_EVENT_COMMAND="$(command -v dutis-event-http)"

dutis-event-http --check --json
dutis watch dutis.toml --once --json
```

The check result reports only whether authentication is configured, never the
endpoint or token.

## Configuration

| Environment variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `DUTIS_HTTP_ENDPOINT` | Yes | — | Absolute `https://` destination |
| `DUTIS_HTTP_BEARER_TOKEN` | No | — | Value for the `Authorization: Bearer ...` header |
| `DUTIS_HTTP_TIMEOUT_SECONDS` | No | `10` | Total request timeout, from 1 to 300 seconds |
| `DUTIS_HTTP_RETRIES` | No | `2` | Curl retry count, from 0 to 10 |

Configuration is deliberately unavailable as Dutis CLI arguments or TOML.
This keeps webhook URLs and credentials out of shell history, plans, policy,
snapshots, audit records, and event payloads. Use a narrowly scoped token and
rotate it independently from Dutis approval tokens.

The adapter uses `/usr/bin/curl`. `DUTIS_HTTP_CURL` can select another absolute
executable for controlled testing, but should normally remain unset.

## Request contract

For each event, the adapter:

- accepts one JSON event on stdin, up to 1 MiB;
- requires the current event schema version and a header-safe event ID;
- sends an HTTPS `POST` with `Content-Type: application/json`;
- sets `X-Dutis-Event-Id`, `X-Dutis-Event-Type`, and `Idempotency-Key` headers;
- optionally sets the Bearer authorization header;
- does not follow redirects and restricts curl to HTTPS;
- disables the user's ambient `.curlrc` before loading its private request
  configuration;
- discards response bodies and exits non-zero for HTTP or transport failures.

Retries can result in repeated delivery when the remote server processes a
request but the response is lost. Consumers should deduplicate using the event
ID or `Idempotency-Key`.

The endpoint and token are written only to an owner-readable temporary curl
configuration file for the lifetime of the request. They are not placed in
curl's process arguments, are removed from curl's child environment, and are
not echoed when delivery fails. The event body uses a separate owner-readable
temporary file. Both files and their private directory are removed after curl
exits.

## LaunchAgent credentials

Dutis intentionally does not copy HTTP credentials into its LaunchAgent
plist. For background delivery, point `DUTIS_EVENT_COMMAND` at a small trusted
wrapper that reads credentials from macOS Keychain and then replaces itself
with the adapter:

```sh
#!/bin/sh
set -eu
export DUTIS_HTTP_ENDPOINT="$(security find-generic-password -a "$USER" -s dutis-http-endpoint -w)"
export DUTIS_HTTP_BEARER_TOKEN="$(security find-generic-password -a "$USER" -s dutis-http-token -w)"
exec /opt/homebrew/bin/dutis-event-http
```

Store the wrapper at an absolute path with owner-only write permissions, make
it executable, and reinstall the Dutis LaunchAgent after setting
`DUTIS_EVENT_COMMAND` to that path. The wrapper contains only Keychain item
names, not credentials.

## Delivery semantics

The adapter is synchronous and has no durable queue. A failure becomes the
same best-effort warning as any other event-command failure; Dutis continues
its primary operation. Failed command deliveries are placed in the durable
outbox and can be retried with `dutis events replay`. See
[durable event replay](event-replay.md).
