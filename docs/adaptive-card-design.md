# Adaptive Card Component Design

This component renders Adaptive Card v1.6 payloads and handles user interactions in a channel-agnostic way. It always emits canonical Adaptive Card JSON plus a small feature summary; channel-specific downsampling is left to `greentic-messaging`.

## Canonical config
- **Component config:** `default_source`, `default_card_inline`, `default_card_asset`, `catalog_registry_ref`, `multilingual`, `language_mode`, `supported_locales`, `direction_mode`, `validation_mode`, `trace_enabled`, `trace_capture_inputs`.
- **Precedence:** explicit invocation input overrides component config, which overrides deprecated env-var compatibility fallbacks, which overrides hardcoded defaults.
- **English baseline:** `en` is always the implicit fallback locale, even when custom locale lists are configured.

## Invocation Envelope
- **Invocation:** `AdaptiveCardInvocation` carrying the card source/spec, explicit payload, optional session metadata, optional state override, optional interaction, and desired mode (`Render`, `Validate`, `RenderAndValidate`).
- **Card source:** inline JSON, an asset path/reference, or a catalog name resolved through inline registry entries, configured `catalog_registry_ref`, or deprecated env-var fallbacks.
- **Context:** `payload`, optional `session`, persisted `state` (from state-store), and optional `template_params` are available for placeholder binding (`@{path}` or `${path}`); whole-string placeholders are replaced with typed values and can specify `||` defaults.
- **Envelope:** Optional `InvocationEnvelope` from `greentic-types` can accompany the invocation for host metadata.
- **Localization:** operation `locale` is the canonical per-call language override; deprecated `i18n_locale` remains compatibility-only.
- **Direction:** the renderer injects Adaptive Card root `lang` and `rtl`, with `direction_mode = auto` treating Arabic locales as RTL.
- **Catalog refs:** `repo://...` is resolved locally in native/test mode; `store://...` requires host/distributor resolution rather than in-component remote fetching.

## Result Structure
- **AdaptiveCardResult:** rendered card (optional for validation-only), optional `AdaptiveActionEvent`, state and session update ops, feature summary, validation issues, and optional telemetry events.
- **Routing:** Actions emit an event with action metadata, inputs, route/verb when available, and card identifiers.

## State & Session Update Model
- **StateUpdateOp:** declarative `Set`, `Merge`, or `Delete` with a dotted path (e.g., `form_data`, `ui.visibility.section`), retained for compatibility.
- **SessionUpdateOp:** route/attribute updates plus simple card stack push/pop hooks for navigation flows.
- Updates are optional/legacy instructions; persistent storage uses `greentic:state/store@1.0.0`.

## Responsibilities
- **In scope:** card resolution (inline/asset/catalog), placeholder binding from context (typed replacement with `||` defaults for whole-string placeholders), minimal expression evaluation (dotted path lookups, interpolation, equality, ternary) via a pluggable engine, structural validation (root type, version present, input ids/uniqueness, action requirements, basic element shape checks), feature analysis, interaction normalization, and declarative updates/events.
- **Out of scope:** channel rendering/downsampling, network calls, or state/session persistence beyond state-store access. The host performs delivery.
