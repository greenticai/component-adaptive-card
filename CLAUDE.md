# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A Greentic WASM component (`wasm32-wasip2`) that renders, validates, and handles interactions for Microsoft Adaptive Card v1.6 payloads. It is channel-agnostic — always emits canonical Adaptive Card JSON plus a feature summary. Channel-specific downsampling and delivery are handled downstream by `greentic-messaging`.

Component ABI: `greentic:component/component@0.6.0`

## Build & Test Commands

```bash
# Full local CI (run before PRs) — the authoritative check
ci/local_check.sh

# Individual steps
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --target wasm32-wasip2 -- -D warnings
cargo test --workspace --all-targets
cargo build --target wasm32-wasip2 --release

# Run a single test
cargo test -- test_name_here

# Makefile shortcuts
make wasm       # release WASM build
make test       # cargo test
make lint       # fmt + clippy (auto-fix, not --check)
make build      # greentic-component build (requires greentic-component CLI)
make check      # greentic-component doctor on built WASM
```

Artifact: `target/wasm32-wasip2/release/component_adaptive_card.wasm`

## Toolchain Requirements

- Rust 1.91+ (pinned in `rust-toolchain.toml`)
- `wasm32-wasip2` target (`rustup target add wasm32-wasip2`)
- Optional: `greentic-component` CLI for `make build`/`make check`/`make flows`
- Optional: `greentic-integration-tester` for README gtests

## Architecture

### Component Entrypoint (`src/lib.rs`)

Exports WIT interfaces for the Greentic component ABI v0.6: descriptor, schema, runtime (`run()`), QA (setup wizard), and i18n. On `wasm32`, WIT bindings are generated via `wit-bindgen`; on native targets, dead code is allowed for library/test use.

All I/O is CBOR-encoded (`greentic_types::cbor::canonical`). Schemas are lazily loaded from embedded `component.manifest.json` and `schemas/` JSON files.

### Core Processing Flow

```
AdaptiveCardInvocation (JSON/CBOR input)
  ├─ render.rs   → resolve card (inline/asset/catalog) → handlebars templating
  │               → placeholder binding → expression eval → feature analysis → validation
  └─ interaction.rs → render card → normalize inputs → generate state/session updates
                    → build AdaptiveActionEvent → persist state → emit telemetry
→ AdaptiveCardResult (rendered card, events, updates, feature summary, validation issues)
```

### Key Modules

| Module | Role |
|--------|------|
| `render.rs` | Card resolution (inline/asset/catalog), handlebars templating, placeholder binding (`@{path\|\|default}`), expression evaluation (`${...}`), feature analysis, structural validation |
| `interaction.rs` | Interaction handling: Submit/Execute merge form data into state, ShowCard/ToggleVisibility update UI state, OpenUrl is passthrough. Emits `AdaptiveActionEvent` with routing info |
| `expression.rs` | Minimal pluggable expression engine (`ExpressionEngine` trait). Supports dotted path lookups, string literals, `==` equality, ternary `? :`. Missing paths fail gracefully |
| `model.rs` | Core types: `AdaptiveCardInvocation`, `CardInteraction`, `AdaptiveCardResult`, `CardFeatureSummary`, `CardSource` (Inline/Asset/Catalog) |
| `config.rs` | `RuntimeConfig` resolution with precedence: explicit invocation > component config > env-var fallback > defaults. Locale resolution and RTL auto-detection |
| `state_store.rs` | Wraps `greentic:state/store@1.0.0` on wasm32, in-memory HashMap for tests. State keyed by `node_id` or `card_instance_id` |
| `asset_resolver.rs` | Pluggable host asset resolution (`AssetResolver` trait). Resolution order: inline registry → env registry → pack assets → host resolver |
| `validation.rs` | JSON schema validation via `jsonschema` crate against `schemas/adaptive-card.invocation.v1.schema.json` |
| `i18n.rs` / `i18n_bundle.rs` | 70+ locales compiled into a CBOR bundle at build time (`build.rs`). Locale chain: `[requested, base_language, en]` |
| `trace.rs` | Optional telemetry events with card source, binding summary, interaction metadata, state hashes (blake3) |

### WIT Definition

`wit/world.wit` defines the component world `component-v0-v6-v0` exporting: `component-descriptor`, `component-schema`, `component-runtime` (the main `run` function), `component-qa`, and `component-i18n`.

### Build Script (`build.rs`)

Packs locale JSON files from `assets/i18n/*.json` into a CBOR bundle embedded via `include_bytes!`. Watches `assets/i18n/` for changes.

## Crate Configuration

- **Crate types**: `cdylib` (WASM) + `rlib` (library for tests)
- **Default feature**: `state-store` (enables `greentic:state/store` integration)
- **`cfg(target_arch = "wasm32")`**: Guards all WIT bindings and WASM-specific code

## Card Resolution Order

1. Inline JSON (`card_spec.inline_json`)
2. Asset path: invocation registry → env registry (`ADAPTIVE_CARD_ASSET_REGISTRY`) → pack assets (`assets/`) → host resolver
3. Catalog name: invocation registry → configured `catalog_registry_ref` → env registry → catalog file (native/test only)

## Git Conventions

Do NOT add Claude co-author attribution to commits or PRs.

## PR Workflow

Per `.codex/global_rules.md`, every PR must:
1. **Pre-PR**: Refresh `.codex/repo_overview.md` to reflect current state
2. **Implement**: Prefer reusing types from `greentic-interfaces`, `greentic-types`, `greentic-secrets`, `greentic-oauth`, `greentic-messaging`, `greentic-events` over defining new ones locally
3. **Post-PR**: Update `.codex/repo_overview.md`, run `ci/local_check.sh`, document any failures in PR summary
