# Testing the Adaptive Card Component

This component supports multiple testing paths, depending on how much of the stack you want to
exercise.

## Option 1: Unit + Conformance Tests (fast)

Run Rust tests directly.

```bash
cargo test
```

## Option 2: Local component test harness (new)

Use the `greentic-component test` harness to invoke the wasm locally with an in-memory
state/secrets store.

1) Build the wasm:

```bash
greentic-component build --manifest ./component.manifest.json --no-flow --no-write-schema
```

2) Invoke the component with inline JSON:

```bash
greentic-component test \
  --wasm target/wasm32-wasip2/release/component_adaptive_card.wasm \
  --manifest ./component.manifest.json \
  --op card \
  --input-json '{
    "card_source": "inline",
    "card_spec": {
      "inline_json": {
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
          { "type": "TextBlock", "text": "Hello {{payload.user.name}}" }
        ]
      }
    },
    "payload": { "user": { "name": "Greentic" } }
  }' \
  --pretty
```

Example with component config defaults:

```bash
greentic-component test \
  --wasm target/wasm32-wasip2/release/component_adaptive_card.wasm \
  --manifest ./component.manifest.json \
  --input-json '{
    "config": {
      "default_source": "inline",
      "default_card_inline": {
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
          { "type": "TextBlock", "text": "Hello {{payload.user.name}}" }
        ]
      },
      "multilingual": true,
      "language_mode": "custom",
      "supported_locales": ["en", "en-GB", "fr", "de", "nl"],
      "direction_mode": "auto"
    },
    "locale": "en-GB",
    "payload": { "user": { "name": "Greentic" } }
  }' \
  --pretty
```

Optional flags:
- `--state-dump` to show the in-memory state after the call.
- `--secret KEY=VALUE` or `--secrets .env` to inject secrets.
- `--flow`, `--node`, `--session` to set execution context.

### Multi-step state setup (new)

The test harness now supports multiple steps in a single run. This is useful for initializing
state and then reading it back in the same in-memory session.

```bash
greentic-component test \
  --wasm ./component.wasm \
  --manifest ./component.manifest.json \
  --op init_state --input ./init.json \
  --step --op read_state --input ./read.json \
  --state-dump
```

### Multi-step adaptive-card example (state + render)

This example writes state via an interaction in the first step, then reads it back on the next
render.

```bash
greentic-component test \
  --wasm target/wasm32-wasip2/release/component_adaptive_card.wasm \
  --manifest ./component.manifest.json \
  --op card --input-json '{
    "card_source": "inline",
    "card_spec": {
      "inline_json": {
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
          { "type": "Input.Text", "id": "comment" }
        ],
        "actions": [
          { "type": "Action.Submit", "title": "Save", "id": "save" }
        ]
      }
    },
    "interaction": {
      "interaction_type": "Submit",
      "action_id": "save",
      "card_instance_id": "card-1",
      "raw_inputs": { "comment": "Hello from state" }
    }
  }' \
  --step --op card --input-json '{
    "card_source": "inline",
    "card_spec": {
      "inline_json": {
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
          { "type": "TextBlock", "text": "Saved: @{state.form_data.comment||\"(none)\"}" }
        ]
      }
    }
  }' \
  --state-dump \
  --pretty
```

## Option 3: Pack + runner smoke test (full stack)

Build a gtpack and run via the runner CLI to verify end-to-end packaging and execution.

```bash
ci/component_pack_smoke.sh
```

## Compatibility notes

- Preferred per-call locale field: `locale`
- Deprecated per-call alias: `i18n_locale`
- Preferred catalog source config: `catalog_registry_ref`
- Deprecated env fallbacks:
  - `ADAPTIVE_CARD_ASSET_BASE`
  - `ADAPTIVE_CARD_ASSET_REGISTRY`
  - `ADAPTIVE_CARD_CATALOG_FILE`
  - `GREENTIC_TRACE`
  - `GREENTIC_TRACE_OUT`
  - `GREENTIC_TRACE_CAPTURE_INPUTS`
