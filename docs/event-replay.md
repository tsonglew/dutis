# Durable event replay

Dutis v2.17 stores failed event-command deliveries in a local outbox. Replaying
an event retries only the external delivery; it never repeats the drift check,
association mutation, policy decision, snapshot, or audit operation that
originally produced the event.

## Storage

When `DUTIS_EVENT_COMMAND` or `--event-command` is configured, the default
outbox is:

```text
$DUTIS_STATE_DIR/event-outbox
```

Without `DUTIS_STATE_DIR`, Dutis uses:

```text
~/Library/Application Support/dutis/event-outbox
```

Set `--event-outbox <directory>` or `DUTIS_EVENT_OUTBOX` to override it. A
custom outbox path is also preserved when installing the drift-monitoring
LaunchAgent.

Each failed delivery is written atomically as one JSON file. The directory is
mode `0700` and records are mode `0600` on Unix. A record contains the original
event, the first and most recent attempt timestamps, and an attempt count. It
does not contain HTTP endpoint or credential configuration. Event payloads can
still contain local paths, bundle identifiers, requester identity, and plans,
so the directory should remain private.

## Inspect pending events

```bash
dutis events pending
dutis events pending --json
```

Pending events are ordered by their first failure time and event ID. Dutis
validates the queue schema, embedded event schema, filename, size, and file
type before returning or delivering a record. Malformed records fail closed
and remain untouched for operator inspection.

## Replay deliveries

First confirm that the command sink and its credentials are healthy. Then run:

```bash
dutis --event-command /absolute/path/to/handler events replay
dutis events replay --limit 25 --json
```

The second form uses `DUTIS_EVENT_COMMAND`. Replay attempts the oldest records
first and processes at most 100 by default. `--limit` changes that bound.
Successful deliveries are removed. Failed deliveries remain in the outbox and
their attempt count increases. A mixed batch exits with partial-failure code
`8`; its JSON error details contain the per-event results.

The original event ID is preserved across every attempt. The bundled HTTP
adapter sends it as both `X-Dutis-Event-Id` and `Idempotency-Key`, allowing a
receiver to deduplicate deliveries.

## Delivery guarantees

Replay is explicit; Dutis does not start a background retry loop. Delivery is
at least once because a process can terminate after the receiver accepts an
event but before the local record is removed. Consumers should deduplicate by
event ID, and operators should run only one replay process for an outbox at a
time.

If the outbox itself cannot be written, Dutis reports that failure with the
event-command warning. The originating command keeps its original result:
observability storage cannot authorize, undo, repeat, or relabel a mutation.
