> **DEPRECATED**: This repository is no longer maintained. Its functionality is superseded by
> [wicked-estate v0.13+](https://github.com/mikeparcewski/wicked-estate). This repository
> will be archived. No further updates will be made.

```
          _      _            _                                                      
__      _(_) ___| | _____  __| |       __ _ _ __  _ __  ___        ___ ___  _ __ ___ 
\ \ /\ / / |/ __| |/ / _ \/ _` |_____ / _` | '_ \| '_ \/ __|_____ / __/ _ \| '__/ _ \
 \ V  V /| | (__|   <  __/ (_| |_____| (_| | |_) | |_) \__ \_____| (_| (_) | | |  __/
  \_/\_/ |_|\___|_|\_\___|\__,_|      \__,_| .__/| .__/|___/      \___\___/|_|  \___|
                                           |_|   |_|                                 
```

**The shared kernel for the wicked-estate-universe apps.** One small crate that `wicked-governance`, `wicked-orchestration`, `wicked-council`, and `wicked-agent` all depend on — the contract between them, defined once.

> **Status:** built · `cargo test` **7 passed** · `clippy -D warnings` clean. The verified seam onto [`wicked-estate`](../wicked-estate) that the four apps program against.

## What it provides
- **`ConformanceClaim` + `Decision`** — the governance verdict governance *produces* and orchestration/agent *consume*.
- **Event catalog + `validate_event_type`** — the `wicked.<noun>.<verb>` names. Coarse, fire-and-forget, **off the hot path**.
- **Node/edge-kind constants** — `POLICY`, `CONFORMANCE_CLAIM`, `WORKFLOW`, `PHASE`, `COUNCIL_TASK`, `AGENT_SESSION`, … for `NodeKind::Other(...)`.
- **`open_store` + the `emit` seam** — open the shared estate `SqliteStore`; emit coarse events with a durable dead-letter spool (the estate emit lives in the binary, so this kernel ships its own).
- **`ToNode` / `FromNode`** — the round-trip that persists a domain entity as an estate `Node` (metadata = JSON), exactly how `wicked-memory` rides estate.

## Why its own repo
Polyrepo needs the shared kernel to have a home all four can pin — like the engines pin [`wicked-estate-core`](../wicked-estate). Depend via path locally; pin a version at release.

## Build
```sh
cargo test                                  # 7 passed
cargo clippy --all-targets -- -D warnings   # clean
```

## License
MIT.
