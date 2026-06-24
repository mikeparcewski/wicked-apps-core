# apps-core — Architecture

The thin shared kernel the four wicked-estate-universe apps (governance, orchestration, council, agent) all program against: estate re-exports, the domain string vocabulary, the cross-app event catalog, the `ConformanceClaim` wire type, the store seam, the emit seam, and the `ToNode`/`FromNode` round-trip — defined once.

## What it provides

The entire public surface, as exported from `src/lib.rs` (and `src/emit.rs`):

| Surface | Public names | What it is |
| --- | --- | --- |
| **Estate node model** (re-export) | `Node`, `NodeKind`, `Edge`, `EdgeKind`, `Language`, `Location`, `Span`, `ResolutionTier`, `Metadata` | The `wicked-estate-core` graph essentials, re-exported so apps build nodes without depending on the estate crate directly. `Metadata` is `serde_json::Map<String, Value>`. |
| **Estate symbol identity** (re-export) | `Symbol`, `SymbolId`, `Descriptor`, `Package`, `Suffix` | Synthetic-symbol construction for domain entities that have no source file. |
| **Estate storage** (re-export) | `GraphRead`, `GraphWrite`, `GraphStore`, `SqliteStore` | The store traits (apps program against these) plus the concrete `SqliteStore` from `wicked-estate-store`. |
| **Node-kind constants** | `POLICY`, `CONFORMANCE_CLAIM`, `WORKFLOW`, `PHASE`, `COUNCIL_TASK`, `COUNCIL_VERDICT`, `CLI_RANKING`, `AGENT_SESSION`, `WORK_UNIT` | `&str` kinds carried via `NodeKind::Other(..)`. Estate's `NodeKind` is a closed enum plus an `Other(String)` escape hatch. |
| **Edge-kind constants** | `GOVERNS`, `DECIDES`, `GATES`, `DISTRIBUTES_TO`, `PRODUCES`, `EVIDENCES` | `&str` relationships carried via `EdgeKind::Other(..)`. `GOVERNS`/`PRODUCES` have native estate equivalents (`EdgeKind::Governs`/`Produces`) — documented on each constant; prefer the native variant where an exact estate-query match matters. |
| **Event catalog** | 18 `EV_*` consts (e.g. `EV_POLICY_REGISTERED`, `EV_WORKFLOW_STARTED`, `EV_COUNCIL_VOTED`, `EV_AGENT_SESSION_STARTED`), `EVENT_CATALOG` (all 18 in declaration order) | The `wicked.<noun>.<verb>` cross-app contract, mirroring `wicked-governance/contracts/events.json`. |
| **Event validation** | `validate_event_type(s) -> bool` | Hand-rolled (no regex) check: `^wicked\.[a-z0-9_]+(\.[a-z0-9_]+)*$`, ≤128 chars. |
| **Governance wire type** | `ConformanceClaim`, `Decision` (`Allow`, `Deny`, `AllowWithConditions`) | The recorded evaluation governance produces. `Decision` serializes `snake_case`; both derive serde + `PartialEq`/`Eq`. |
| **Store seam** | `open_store(path: Option<&str>) -> anyhow::Result<SqliteStore>`, `ESTATE_DB_ENV` (`"WICKED_ESTATE_DB"`) | Opens the shared estate DB (see below). |
| **Round-trip traits** | `ToNode` (`node_kind()` + `to_node()`), `FromNode` (`from_node()`), `synthetic_symbol(kind, id) -> SymbolId`, `SYMBOL_SCHEME` (`"wicked-apps"`) | The domain ↔ estate-`Node` mapping pattern. `SamplePolicy` is the in-crate proof entity used only by the round-trip test. |
| **Emit seam** (`emit` module) | `EmitEvent` + `EmitEvent::new`, `emit_event(&EmitEvent) -> bool`, `deadletter_path()`, `DEADLETTER_ENV`, `EMIT_PROGRAM_ENV`, `DEADLETTER_MARKER` | The event-publish path (see below). |

## The emit seam

`src/emit.rs` is apps-core's own copy of the estate emit seam. It exists because estate's `emit` is declared `mod emit;` in the estate **binary** (`main.rs`), so it is not part of the `wicked-estate` library API — a path dependency cannot import it. apps-core therefore mirrors that seam's shape and contract for the apps domain.

- **Shape.** `EmitEvent { event_type, domain, subdomain, payload }`, built with `EmitEvent::new(event_type, domain, subdomain, payload)` where `payload` is a `serde_json::Value`. `emit_event(&event)` spawns the canonical `wicked-bus emit` CLI (`--type/--domain/--subdomain/--payload`), with the program overridable via `EMIT_PROGRAM_ENV` (`WICKED_APPS_EMIT_PROGRAM`).
- **Fire-and-forget, never silent.** Emit must never block or fail the caller, so it returns `bool` rather than erroring: `true` if the bus child exited zero, `false` if the event was dead-lettered. The failure path is loud and durable — if the CLI can't be spawned or exits non-zero, `dead_letter(..)` appends one NDJSON line (type, domain, subdomain, payload, `deadletter_reason`) to a spool and writes the greppable `DEADLETTER_MARKER` (`"EMIT-DEADLETTER:"`) to stderr. A dropped event is a defect, never a silent loss.
- **Spool path.** `deadletter_path()` resolves `DEADLETTER_ENV` (`WICKED_APPS_EMIT_DEADLETTER`) if set, else `<home>/.something-wicked/wicked-apps/emit-deadletter.ndjson`. Home is resolved cross-platform via `HOME` / `USERPROFILE` joined with `Path` segments — never a hardcoded `~`.
- **Coarse, off the hot path.** Events are coarse `wicked.<noun>.<verb>` signals published out-of-band by shelling to a CLI, not a tight in-process loop.

## Estate mapping pattern

apps-core does not define per-app storage. Domain entities ride the shared estate graph:

- An entity persists as a `Node` whose kind is `NodeKind::Other(kind)` — `kind` being a node-kind constant such as `POLICY`. Its stable, round-trippable fields go into `Node.metadata` (a JSON map). Relationships between entities are `Edge`s with `EdgeKind::Other(..)` (or a native estate variant).
- Each entity gets a stable, source-file-free identity via `synthetic_symbol(kind, id)`, which builds a `SymbolId` from `Symbol::synthetic(SYMBOL_SCHEME, "{kind}/{id}")`. Namespacing by kind means a policy `p1` and a workflow `p1` never collide.
- An entity implements `ToNode` (`node_kind()` + a lossless `to_node()`) and `FromNode` (`from_node()` rebuilds it, validating the `NodeKind::Other` tag and pulling load-bearing fields back out of metadata). The contract is that `to_node()` is lossless w.r.t. `from_node()`.
- Persistence is the shared `SqliteStore` via the `GraphWrite` batch path (`begin_batch` → `upsert_nodes` → `commit_batch`) and `GraphRead::get_node`. The in-crate test rounds `SamplePolicy` through a real in-memory store to prove identity is preserved.

This is the same pattern `wicked-memory` uses to ride estate: a domain object encoded as an estate `Node` with its state in metadata, rather than a bespoke schema.

## Why its own repo

apps-core is a polyrepo shared kernel — the one contract all four apps pin, the way the engines pin `wicked-estate-core`. Locally the apps depend on it via a `path` dependency; at release each app pins a published version. Keeping it in its own repo gives that contract a single home and a single version to bump.

## Build

```sh
cargo test                                  # 7 passed
cargo clippy --all-targets -- -D warnings   # clean
```

The seven tests are non-vacuous: `validate_event_type` accept/reject, the `SamplePolicy` → `Node` → `SqliteStore` → `SamplePolicy` round-trip, the `:memory:` store path, `ConformanceClaim` serde, and (in `emit`) the dead-letter spool + the cross-platform default spool path.

> **Validator quirk worth knowing.** `EVENT_CATALOG` contains `wicked.phase.ready-for-gate` (`EV_PHASE_READY_FOR_GATE`), whose hyphen is **not** admitted by `validate_event_type`'s `[a-z0-9_]` grammar. This is intentional and tested: the validator enforces the stated grammar strictly, and the hyphenated name is a documented catalog exception that it rejects. Don't gate that one event on `validate_event_type`.
