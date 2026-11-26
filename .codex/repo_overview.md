# Repository Overview

## 1. High-Level Purpose
- Greentic Adaptive Card component that renders canonical Adaptive Card v1.6 JSON, reports feature usage, and handles interactions with declarative state/session updates and routing events.
- Rust + WASI-P2 component using Greentic node guest bindings; channel-specific downsampling is intentionally left to `greentic-messaging`.

## 2. Main Components and Functionality
- **Path:** src/lib.rs  
  **Role:** Component guest implementation and entrypoint glue.  
  **Key functionality:** Exposes component-node exports; parses invocation JSON (optionally wrapped) into `AdaptiveCardInvocation`; routes to render or interaction handling; serializes `AdaptiveCardResult` or error payloads.
- **Path:** src/model.rs  
  **Role:** Invocation, interaction, update op, feature summary, and result data models.  
  **Key functionality:** Defines `CardSource`, `CardSpec`, `InvocationMode`, `CardInteraction`, `AdaptiveActionEvent`, `StateUpdateOp`, `SessionUpdateOp`, `CardFeatureSummary`, and `AdaptiveCardResult`; integrates optional `greentic_types::InvocationEnvelope`.
- **Path:** src/render.rs  
  **Role:** Card resolution, binding, feature analysis, and validation.  
  **Key functionality:** Resolves cards from inline/asset/catalog sources (inline/env registries, pack assets under `ADAPTIVE_CARD_ASSET_BASE`, optional host resolver fallback); expands array-element `$data: "${path}"` runtime collections with nested item scopes, then applies binding via a minimal expression engine (dotted path lookups over payload/session/state/params, `@{}`/`${}` interpolation, equality `==`, ternary); analyzes used elements/actions and feature flags; performs structural validation (root type, version presence, input/action ids/uniqueness, required action fields, array/choice/column/media checks, Execute data typing, duplicate action ids).
- **Path:** src/interaction.rs  
  **Role:** Interaction normalization and event/update generation.  
  **Key functionality:** Builds `AdaptiveActionEvent`, normalizes inputs into `event.inputs` (including submitted row IDs and action/nextCardId data), emits state/session update ops for Submit/Execute/ShowCard/ToggleVisibility/OpenUrl, and returns rendered card/features/validation.
- **Path:** src/expression.rs  
  **Role:** Pluggable minimal expression evaluator.  
  **Key functionality:** Defines `ExpressionEngine` trait and `SimpleExpressionEngine` supporting dotted path lookups, interpolation, equality, ternary, and graceful failure.
- **Path:** src/asset_resolver.rs  
  **Role:** Host asset resolver abstraction.  
  **Key functionality:** Defines `AssetResolver` trait with map/callback implementations and registration helpers; `resolve_with_host` queries an optional host resolver used after local resolution sources.
- **Path:** docs/adaptive-card-design.md  
  **Role:** Design notes and responsibility split with messaging.  
  **Key functionality:** Documents invocation envelope, result shape, update model, asset resolution order, and minimal expression scope.
- **Path:** tests/conformance.rs  
  **Role:** Integration tests.  
  **Key functionality:** Cover runtime collection repetition, nested rows, per-row Submit response IDs, empty/missing/invalid collections, expanded action validation, inline/asset render, binding/interpolation, feature summary flags, Submit/Toggle interactions, host asset resolver callback, and validation of describe payload.
- **Path:** schemas/  
  **Role:** JSON schemas for component config and I/O.  
  **Key functionality:** Input schema for Adaptive Card invocations; output schema for `AdaptiveCardResult`; component config exposes optional asset base path.
- **Path:** ci/local_check.sh  
  **Role:** Local CI wrapper.  
  **Key functionality:** Prefetches canonical WIT, runs formatting, native/WASM clippy and native tests, builds and syncs release WASM, checks state capability wiring, and optionally runs README gtests.
- **Path:** Makefile  
  **Role:** Convenience tasks.  
  **Key functionality:** Build/check/lint/test targets using cargo (wasm32-wasip2 target for build/check).
- **Path:** component.manifest.json  
  **Role:** Component manifest for Greentic runtime.  
  **Key functionality:** Describes component id, capabilities, artifact path, and current wasm hash.

## 3. Work In Progress, TODOs, and Stubs
- **Location:** src/render.rs & src/expression.rs  
  **Status:** Intentional limitation  
  **Short description:** Expression engine is intentionally minimal (paths/interpolation/equality/ternary). Validation remains structural; full Adaptive Card expression-language parity and deeper type checks are deferred.

## 4. Broken, Failing, or Conflicting Areas
- Runtime collection changes: all 70 tests pass with `RUST_TEST_THREADS=1`; formatting and native/WASM clippy pass. Parallel conformance execution intermittently fails the existing `catalog_resolution_uses_env_mapping` test (shared asset configuration). Release WASM build passes. `ci/local_check.sh` stops at dist sync because macOS Bash 3.2 lacks `mapfile`; dist remains unchanged. Host-state capability and all three README gtests pass when run directly.

## 5. Notes for Future Work
- Consider fuller Adaptive Card expression support once a shared engine is available, keeping the pluggable boundary.
- Extend validation to cover deeper Adaptive Card semantics and type enforcement.
- Formalize host asset resolver against a future shared WIT/API if introduced.
