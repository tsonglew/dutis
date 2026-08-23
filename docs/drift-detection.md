# Drift detection

Dutis can compare current macOS file associations with a declared TOML
configuration once or continuously. Detection is read-only by default.

## One-time report

```bash
dutis watch dutis.toml --once
dutis watch dutis.toml --once --json
```

Each report includes:

- a schema version and UTC check time;
- `in_sync`, `drift_detected`, or `unresolved` state;
- changed and unresolved entries with current and target applications;
- the complete deterministic plan and digest;
- the effective policy summary and remediation assessment.

JSON mode emits one compact object per line, so the same output works for a
single check and long-running log ingestion.

## Continuous monitoring and notifications

```bash
dutis watch dutis.toml --interval-seconds 60 --notify
```

During one watcher session, macOS notifications are deduplicated. Dutis
notifies when drift first appears, when its plan changes, or when associations
return to the declared state. Every check is still written to stdout.

Every check can also produce a versioned `drift.checked` event through a JSONL
or command sink:

```bash
dutis --event-log ./events.jsonl watch dutis.toml --interval-seconds 60
```

Unlike desktop notifications, event sinks receive every check so external
automation can use them as both state changes and monitoring heartbeats. See
[event sinks](event-sinks.md).

## Optional LaunchAgent

Install a per-user background monitor:

```bash
dutis launch-agent install dutis.toml --interval-seconds 300 --notify
dutis launch-agent status --json
```

The installed service is named `io.github.tsonglew.dutis.watch`. It stores an
absolute path to the Dutis executable and configuration, runs the continuous
watcher, and writes:

```text
~/Library/Application Support/dutis/logs/watch.jsonl
~/Library/Application Support/dutis/logs/watch.error.log
```

If `DUTIS_STATE_DIR` is set while installing, logs and state use that directory.
If `--event-log`, `--event-command`, `DUTIS_EVENT_LOG`, or
`DUTIS_EVENT_COMMAND` is set while installing, the normalized sink paths are
copied into the LaunchAgent environment. Reinstall the agent after changing a
sink.
The LaunchAgent plist is stored at:

```text
~/Library/LaunchAgents/io.github.tsonglew.dutis.watch.plist
```

Replace an existing agent by running `install` again. Remove it with:

```bash
dutis launch-agent uninstall
```

The configuration file must remain at the recorded absolute path. Reinstall the
agent after moving the file or installing Dutis at a different location.

## Automatic remediation

Automatic remediation is deliberately opt-in:

```bash
dutis watch dutis.toml \
  --interval-seconds 60 \
  --remediate --yes \
  --requester local-watch
```

For a background agent:

```bash
dutis launch-agent install dutis.toml \
  --interval-seconds 300 \
  --notify \
  --remediate --yes \
  --requester local-launch-agent
```

Every remediation rebuilds the plan from current state and passes through the
same policy, audit, pre-change snapshot, mutation, and verification pipeline as
`dutis apply`. Policy denial and failures are reported without disabling later
checks. Unresolved plans are never remediated.

Foreground token-policy remediation reads the token only from
`DUTIS_WATCH_APPROVAL_TOKEN`. The token is never printed or accepted as a
command-line argument. Remediating LaunchAgents require the explicit approval
policy because Dutis will not persist an approval token in a plist. Use a
notification-only agent when token approval is required.

## MCP

The read-only `dutis_drift` MCP tool accepts inline `config_toml` and returns the
same report. No watcher remediation tool is registered. Agents must use the
reviewed `dutis_diff`, `dutis_policy_check`, and gated `dutis_apply` flow when a
change is requested.
