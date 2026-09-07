# Operation Events and SSE Design

## Goal

Expose review/apply progress to HTTP, WebUI and GUI clients without repeating an operation after reconnect. Progress is durable machine-local operation state; SSE is only a transport over the shared application request.

## Durable event log

Each operation directory may contain `events.json` beside its plan, progress and result files. The document uses schema `v1.0`, names the operation ID and contains an ordered, bounded event array. Event IDs are positive, monotonically increasing integers scoped to one operation.

An event contains:

- `id`: reconnect cursor;
- `kind`: `planned`, `applying`, `progress`, `recovering`, `applied` or `failed`;
- `completed` and `total` when measurable;
- `message`: concise adapter-neutral status;
- `recorded_at`: RFC 3339 timestamp.

Plan creation records `planned`. Apply records `applying`, then records progress only after the corresponding recovery journal is durable. Interrupted-operation recovery records `recovering`. A durable result records `applied`. An ordinary apply failure records `failed` only after any required restore attempt; the domain error remains the authoritative failure response.

Appending an event rewrites `events.json` through the existing atomic replacement path. A duplicate transition with the same kind and progress counters is ignored, so retrying recovery or a completed operation does not create unbounded duplicates. Large write sets record the first step, approximately one-percent milestones and completion rather than one event per file. Logs have a hard event-count cap. Existing operation directories without a log return a synthetic current-state event in memory and are not mutated by inspection.

Event logging is part of operation persistence. Before the Vault changes, failure to save an event aborts apply. During an apply, progress is journaled before its event; an event-write failure follows the operation's existing restore/recovery path rather than leaving an untracked Vault change. If the result exists but the final event does not, retrying the operation appends the missing `applied` event and returns the existing result.

## Shared application request

`kb-app` provides a fixed-Vault operation-events request. It verifies that the operation belongs to the selected Vault and returns the ordered events plus a `terminal` flag. The request does not wait, open sockets or know SSE.

## HTTP SSE

```text
GET /operations/{operation_id}/events
Last-Event-ID: <positive integer>
Accept: text/event-stream
```

The route uses the same authentication and fixed-Vault checks as operation inspection. It sends only events whose ID is greater than `Last-Event-ID`. Each SSE event has `id`, event name `operation`, and a JSON data object using the shared event schema. Invalid or future cursors return the normal JSON `invalid_config` error before streaming starts.

The adapter polls the shared event request at a bounded interval. It closes after the terminal event has been delivered. Transport keepalive comments may be sent but are not operation events. Disconnecting cancels only that subscriber; it does not cancel or repeat apply. Multiple subscribers do not own the operation.

## Boundaries

Operation events are machine-local execution metadata and are excluded from Vault backups. They do not contain source text, generated Markdown, diffs, tokens or arbitrary paths. The first slice does not provide cancellation, background job creation, WebSocket transport, cross-machine event replication or retention cleanup.

Focused tests cover event ordering and idempotency, interruption/retry transitions, fixed-Vault inspection, SSE framing, reconnect cursors, terminal close and authentication. Full-suite, release, native Windows/Linux, browser EventSource and slow-client load testing remain unverified.
