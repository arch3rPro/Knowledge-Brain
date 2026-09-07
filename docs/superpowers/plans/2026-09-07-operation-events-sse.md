# Operation Events and SSE Implementation Plan

**Goal:** Add durable operation progress and reconnectable SSE without moving business behavior into the HTTP adapter.

**Architecture:** `kb-core` owns event types, `kb-app` atomically records and reads per-operation logs, and `kb-server` converts repeated fixed-Vault snapshots into authenticated SSE.

**Spec:** `docs/superpowers/specs/2026-09-07-operation-events-sse.md`

**Verification constraint:** Run focused tests only; do not run the workspace-wide suite.

### Task 1: Event contract and storage

- [x] Define schema-versioned event/log/report types and validation.
- [x] Atomically append bounded, idempotent events in operation directories.
- [x] Return a synthetic current-state event for legacy operations without mutating them.

### Task 2: Operation lifecycle integration

- [x] Record plan creation for adoption, source capture and knowledge save.
- [x] Record applying, durable progress, recovery, failure and completion transitions.
- [x] Verify process-exit recovery and completed-operation retry do not duplicate transitions.

### Task 3: Shared read request

- [ ] Add a fixed-Vault application request returning events and terminal state.
- [ ] Reject operation IDs from another Vault without exposing their data.
- [ ] Test plan, active and completed reports through the real application dispatcher.

### Task 4: SSE adapter

- [ ] Add authenticated `GET /operations/{id}/events`.
- [ ] Support `Last-Event-ID`, terminal close and transport-only keepalive comments.
- [ ] Test real TCP SSE framing, reconnect filtering, auth and fixed-Vault rejection.

### Task 5: Documentation and focused verification

- [ ] Publish the event/SSE reference, security boundary, capability and Stage 4B status.
- [ ] Run format, related crate Clippy, focused app/server/CLI tests and diff checks.
- [ ] Record unverified browser/load/platform/full-suite limits explicitly.
