# Development

Channel-agnostic Adaptive Card engine for Greentic components. It renders Adaptive Card v1.6 payloads (inline, asset, or catalog), applies simple placeholder binding from flow/session/state, validates structure, and handles user interactions. Outputs include:
- Canonical Adaptive Card JSON plus a feature summary (for `greentic-messaging` downsampling).
- Declarative state/session update operations.
- Optional routing events describing the triggered action.

## Requirements
- Rust 1.91+
- `wasm32-wasip2` target (`rustup target add wasm32-wasip2`)
- Component ABI `0.6.0` (`greentic:component/component@0.6.0`)
- `greentic-interfaces-guest` with `features = ["component-v0-6"]` for wasm builds

## Development
```bash
cargo fmt --all
cargo test --workspace --all-targets
cargo build --target wasm32-wasip2
# or run everything via the local CI wrapper
ci/local_check.sh
```

`component.manifest.json` references the release artifact at `target/wasm32-wasip2/release/component_adaptive_card.wasm`. Update the manifest hash with:
```bash
greentic-component inspect --json target/wasm32-wasip2/release/component_adaptive_card.wasm
```

## Behaviour
- **Invocation:** see the inlined `operations[].input_schema` for `card` in `component.manifest.json`; optional `greentic_types::InvocationEnvelope` metadata can be included.
- **Results:** see the inlined `operations[].output_schema` for `card` in `component.manifest.json` for the result shape (rendered card, events, updates, feature summary, validation issues).
- **Assets:** card assets resolve in order: inline JSON (when provided), inline/env registries (`asset_registry` map or `ADAPTIVE_CARD_ASSET_REGISTRY`), pack assets under `ADAPTIVE_CARD_ASSET_BASE` (default `assets/`), and an optional host asset resolver registered via `register_host_asset_*`. Catalog names map to `<base>/<name>.json` after registry lookups.
- **Binding & expressions:** placeholders support typed replacement with `||` defaults for whole-string bindings (e.g., `@{session.user.name||"Guest"}`); `${...}` expressions use a minimal pluggable engine supporting dotted path lookups over payload/session/state/params, string interpolation, equality (`==`), and ternary selection. Missing paths fail gracefully.
- **Design notes:** `docs/adaptive-card-design.md` captures the component responsibilities and feature summary contract.

Channel-specific downsampling and delivery are handled by `greentic-messaging`; this component always emits canonical Adaptive Card JSON.
