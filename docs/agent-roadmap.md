# Dutis Agent Roadmap

This roadmap evolves Dutis from an interactive macOS utility into a safe,
composable capability for people, scripts, and AI agents. Each phase should be
useful on its own and must preserve the existing interactive experience.

## Product principles

- Mutations are never implicit. Read operations and write operations remain
  separate, and non-interactive writes require explicit confirmation.
- Every write follows `plan -> apply -> verify`. Dry runs must not invoke a
  mutating system command.
- Machine output is versioned. JSON stdout contains only the response envelope;
  warnings and diagnostics go to stderr.
- Selectors resolve deterministically. Ambiguous application names fail instead
  of choosing an arbitrary installation.
- Autonomous enforcement comes only after snapshots and rollback are available.
- The local machine remains the source of truth. Recommendations never silently
  override user policy.

## Phase 1: Agent-ready CLI

Status: implemented for v2.5.0

Deliver a stable command interface while keeping `dutis` without arguments as
the interactive mode.

Commands:

```text
dutis list [--json]
dutis query <extension> [--json]
dutis get <extension> [--json]
dutis set <extension> <bundle-id|path|name> [--dry-run] [--yes] [--json]
dutis doctor [--json]
```

Acceptance criteria:

- JSON responses include an `api_version`, command name, and data or error.
- `set --dry-run` resolves the target and prints the exact planned command
  without changing Launch Services.
- A real non-interactive `set` requires `--yes` and verifies the resulting
  association.
- Bundle IDs and exact paths are preferred selectors; names must be unique.
- Exit codes distinguish usage (`2`), not found (`3`), ambiguity (`4`), missing
  dependencies (`5`), and operation failure (`6`).
- Unit tests cover parsing, normalization, selection, JSON envelopes, and duti
  output parsing. CI runs format, Clippy, and all tests.

## Phase 2: Declarative configuration

Status: implemented for v2.6.0

Add a checked-in `dutis.toml` format and idempotent workflows:

```text
dutis plan dutis.toml
dutis diff dutis.toml
dutis apply dutis.toml --yes
```

The plan contains current state, desired state, proposed changes, unresolved
applications, and a deterministic plan digest. `apply` rejects stale plans and
returns per-item verification results.

Acceptance criteria:

- Reapplying a converged configuration makes no changes.
- Partial failures are reported per association and produce a non-zero exit.
- Config and result schemas have documented versions and migration rules.

## Phase 3: Snapshots, history, and rollback

Status: implemented for v2.7.0

Capture associations before every multi-item apply and expose:

```text
dutis snapshot create
dutis history
dutis rollback <snapshot-id> --dry-run
dutis rollback <snapshot-id> --yes
```

Acceptance criteria:

- Snapshots are local, inspectable, and do not contain credentials.
- Rollback uses the same plan/apply/verify pipeline.
- Failed applies retain enough state to recover safely.

## Phase 4: MCP server and agent tools

Status: implemented for v2.8.0

Expose the core library through a local MCP server. Keep read tools (`list`,
`query`, `get`, `diff`) separate from mutation tools (`apply`, `rollback`). Write
tools accept a plan digest and explicit approval token rather than free-form
shell commands.

Acceptance criteria:

- An agent can discover installed handlers and produce a plan without write
  permission.
- Write capabilities can be disabled independently.
- Tool schemas, errors, and audit events are stable and tested.

## Phase 5: Agent skill and policy layer

Status: implemented for v2.9.0

Ship a Dutis skill for common workflows and add local policy controls such as
allowed extensions, allowed applications, protected associations, and required
approval modes. Record who requested each change and which verified plan was
used.

Acceptance criteria:

- Policy denial happens before a system command executes.
- Every mutation has a local audit record with plan, result, and verification.
- The skill defaults to inspection and dry-run before requesting approval.

## Phase 6: Profiles and recommendations

Status: implemented for v2.10.0

Add reusable profiles such as developer, designer, media, and minimal macOS,
plus explainable recommendations based on installed applications and current
associations. Recommendations remain proposals and include their evidence.

Acceptance criteria:

- Profiles can be inspected without `duti` or write permission.
- Recommendations explain candidate priority, installation evidence, extension
  support, current-handler retention, and unavailable choices.
- Every proposal includes TOML, a deterministic plan digest, and the effective
  policy assessment without changing system associations.
- CLI and MCP expose the same read-only profile workflow.

## Phase 7: Drift detection

Status: implemented for v2.11.0

Add `dutis watch` and an optional LaunchAgent to detect differences from a
declared policy. Start with notifications and reports. Automatic remediation is
opt-in and requires rollback-ready snapshots.

Acceptance criteria:

- One-shot and continuous checks emit timestamped, versioned drift reports.
- Notifications are sent only when drift changes or the monitored state
  recovers during a watcher session.
- The optional per-user LaunchAgent uses an absolute executable and config path,
  keeps secrets out of its plist, and records JSON Lines logs.
- Remediation requires explicit opt-in and a requester identity, then passes
  through policy, audit, safety snapshot, mutation, and verification.
- MCP exposes drift inspection as a read-only tool and does not add an
  autonomous write tool.

## Phase 8: Broader association support

Status: implemented for v2.12.0

Expand the model beyond filename extensions to URL schemes, UTIs, MIME types,
and Launch Services roles. Preserve one normalized planning and verification
pipeline across all association types.

Acceptance criteria:

- Configuration schema version 2 accepts extension, UTI, MIME, and URL-scheme
  targets while version 1 extension configurations remain compatible.
- CLI and MCP expose typed read operations; declarative diff/apply supports all
  types through the existing digest, policy, snapshot, audit, and verification
  boundary.
- Policies can allow kinds and protect a kind/identifier/role tuple.
- URL schemes reject document roles and use the role-free `duti` invocation.
- Unit and fake-`duti` integration tests cover normalization, planning, dry-run,
  mutation arguments, snapshotting, auditing, and verification.

## Phase 9: Event sinks

Status: implemented for v2.13.0

Expose versioned drift and mutation lifecycle events without requiring
automation to scrape terminal output or audit directories.

Acceptance criteria:

- Global CLI options and environment variables configure private JSONL and
  trusted local-command sinks.
- Every drift check emits a `drift.checked` event; governed mutations emit
  pending, denied, failed, and completed lifecycle events after the
  corresponding durable audit update.
- Event commands receive exactly one JSON object on stdin, with stdout
  discarded so CLI and MCP protocol output cannot be corrupted.
- Sink delivery is best effort and cannot bypass policy, suppress audit
  storage, or change the result of a completed mutation.
- LaunchAgent installation preserves normalized sink paths without persisting
  approval tokens.

## Phase 10: Typed application metadata

Status: implemented for v2.14.0

Extend application discovery beyond filename extensions while keeping handler
registrations distinct from UTI definitions.

Acceptance criteria:

- Parse role-aware document registrations for extensions, UTIs, and MIME types,
  plus URL-scheme registrations.
- Parse imported and exported UTI identifiers, conformance, filename tags, and
  MIME tags as separate type-definition evidence.
- Preserve the legacy `extensions` field while adding stable typed metadata to
  application JSON.
- CLI and MCP expose compact typed handler queries with exact matching
  declarations and normalized identifiers.
- Role matching recognizes that an editor can satisfy a viewer query, while
  shell and editor capabilities remain distinct.

## Phase 11: HTTPS event adapter

Status: implemented for v2.15.0

Ship an optional network adapter as an external event command while keeping
transport secrets outside Dutis configuration, plans, and audit records.

Acceptance criteria:

- `dutis-event-http` receives one current-schema event on stdin and delivers an
  HTTPS POST without gaining any mutation capability.
- Endpoints, Bearer credentials, timeouts, and retries are configured only by
  adapter-specific environment variables.
- Endpoint and credential values never appear in child process arguments,
  check output, or delivery errors; private temporary request files are removed
  after delivery.
- Requests include event identity and idempotency headers, enforce a bounded
  input size, reject plaintext HTTP, do not follow redirects, and discard
  response bodies.
- Release archives and the Homebrew formula install both universal binaries;
  fake-transport tests cover success, sanitization, and rejection paths.

## Phase 12: Native role-aware reads

Status: implemented for v2.16.0

Use macOS Launch Services directly where `duti` cannot distinguish default
handlers by role, while retaining `duti` as the mutation backend.

Acceptance criteria:

- Resolve extensions and MIME types to UTIs through Core Services and query
  content defaults for `all`, `viewer`, `editor`, and `shell` roles.
- Query URL-scheme defaults through the native Launch Services API.
- CLI and MCP expose a read-only default-role matrix with explicit null values
  for roles that have no registered default.
- Role-specific get, planning, drift, snapshot, and post-write verification use
  native reads by default so verification observes the requested role.
- An explicit backend override supports compatibility diagnostics without
  weakening mutation, policy, audit, or snapshot boundaries.

## Phase 13: Durable event replay

Status: implemented for v2.17.0

Persist failed external event-command deliveries locally and let operators or
agents retry them without re-running the originating association operation.

Acceptance criteria:

- Command-delivery failures are atomically queued with the original event ID,
  payload, timestamps, and attempt count in owner-only local storage.
- CLI commands list pending deliveries and replay a bounded oldest-first batch
  through the currently configured command sink.
- Successful deliveries are removed; failed deliveries remain queued and
  increment their attempt count.
- Replay retains the original event ID for receiver-side idempotency, validates
  stored records before delivery, and rejects invalid IDs or non-regular queue
  entries.
- Queue failures and replay failures never change mutation, policy, audit,
  snapshot, or drift outcomes.

## Phase 14: Policy-aware fleet recommendations

Status: implemented for v2.18.0

Use locally deployed team preferences and existing mutation constraints while
ranking profile candidates, without adding a remote policy service.

Acceptance criteria:

- The version-one policy schema accepts ordered global and per-extension
  recommendation preferences while remaining backward compatible.
- Recommendation ranking honors protected targets, application/extension/kind
  allowlists, extension preferences, and global preferences before profile
  defaults.
- Candidate evidence reports its source, policy eligibility, and concrete
  exclusion reasons; blocked associations never enter proposed configuration.
- CLI and MCP use the same effective local policy and return a plan that can be
  independently rechecked by the existing governed workflow.
- MCP callers cannot inject or replace policy, and no network control plane or
  new mutation path is introduced.

## Phase 15: Event retention and dead-letter cleanup

Status: implemented for v2.19.0

Add explicit lifecycle controls for delivery failures while keeping recovery
safe, local, and operator initiated.

Acceptance criteria:

- Pending events can be selected by delivery-attempt count, queue age, or
  either threshold in a combined retention rule.
- Archive and purge are preview-only by default and require `--yes` before
  changing local state.
- Archiving atomically preserves the complete pending record, original event
  ID, timestamps, attempt count, and matching reasons in private dead-letter
  storage.
- Purge permanently removes only dead letters older than an explicit positive
  retention period; it never deletes pending deliveries.
- Malformed, oversized, mismatched, or non-regular records fail closed, and no
  maintenance command is exposed through MCP.

## Phase 16: Typed fleet recommendation preferences

Status: implemented for v2.20.0

Extend locally deployed recommendation preferences across every association
kind without creating a remote policy or mutation path.

Acceptance criteria:

- The version-one policy schema accepts ordered UTI, MIME, and URL-scheme
  recommendation handlers with normalized identifiers and roles while
  remaining backward compatible.
- Typed preferences participate in the same protected-target, kind, and
  application allowlists as extension recommendations.
- A typed candidate must be uniquely installed and explicitly declare the
  requested handler and compatible role before it can enter a proposal.
- CLI and MCP query current defaults through the role-aware backend and return
  typed proposed configuration, deterministic plan, candidate evidence, and
  policy assessment without adding a write capability.
- Recommendation JSON retains its legacy `extension` identifier and
  `declares_extension` fields while additive typed fields identify every target
  consistently.

## Phase 17: Event delivery health summaries

Status: implemented for v2.21.0

Provide compact operational visibility into the local event outbox without
exposing the event bodies that may contain paths, identities, or plans.

Acceptance criteria:

- CLI and read-only MCP expose the same versioned health summary without event
  IDs, payloads, retention reasons, or local storage paths.
- Health includes pending and dead-letter counts, total and maximum attempts,
  timestamp ranges, and stable breakdowns by event type and source.
- Status distinguishes an empty healthy outbox, a retryable pending backlog,
  and dead letters that require operator attention.
- Metrics reuse strict outbox validation and fail closed on malformed,
  oversized, mismatched, or non-regular records.
- Health inspection never replays, archives, purges, or otherwise changes an
  event record.

## Phase 18: Configurable profile overlays

Status: implemented for v2.22.0

Let teams extend built-in recommendations or add reusable local profiles
without recompiling Dutis or weakening the policy boundary.

Acceptance criteria:

- A strict version-one local overlay schema supports extending and replacing
  built-in profiles plus defining named custom profiles.
- Overlay associations use the same normalized extension, UTI, MIME,
  URL-scheme, and role model as declarative configuration.
- Overlay candidate order is deterministic, duplicate applications and targets
  fail closed, and typed candidates still require matching bundle metadata.
- CLI and MCP load the same effective profile catalog; MCP callers cannot
  inject overlays through tool arguments.
- Missing explicitly configured files, unsupported versions, oversized files,
  symlinks, and malformed entries fail before recommendation.

## Phase 19: Guided terminal experience

Status: implemented for v2.23.0

Make the no-argument human workflow reflect Dutis's safety model without
introducing a full-screen TUI or new runtime dependency.

Acceptance criteria:

- A numbered main menu separates extension management, application browsing,
  system readiness, and advanced-workflow discovery.
- Extension inspection shows the current default before presenting matching
  applications.
- Interactive mutations display the current and proposed handler and require
  an explicit confirmation that defaults to no.
- Cancelling, pressing Enter at confirmation, or returning from a submenu does
  not change system state.
- The governed policy, safety snapshot, audit, mutation, and verification
  pipeline remains shared with non-interactive commands.
- Input parsing and the guided entry point are covered by automated tests.

## Phase 20: Keyboard-first terminal UI

Status: implemented for v2.24.0

Add a compact interaction model inspired by modern coding-agent terminals while
preserving the stable CLI and non-TTY behavior.

Acceptance criteria:

- Real terminals provide arrow-key navigation, highlighted selections, Enter
  activation, and Esc or `q` cancellation without entering an alternate screen.
- Long application lists support live fuzzy filtering by application name and
  bundle identifier.
- Confirmation remains explicit and defaults to no before any governed
  mutation.
- Piped input, redirected output, and `TERM=dumb` automatically use the
  numbered plain-text flow.
- `DUTIS_TUI=plain` provides a documented manual fallback for accessibility,
  recording, and terminal compatibility.
- TUI capability selection is tested independently from the host terminal.

## Near-term engineering sequence

1. Release Phase 20 and observe keyboard navigation and search behavior across
   common macOS terminals.
2. Evaluate bounded concurrency controls for replay after observing real sink
   latency and failure patterns.
3. Add signed profile distribution only if local deployment workflows prove
   insufficient; do not introduce an implicit remote control plane.
