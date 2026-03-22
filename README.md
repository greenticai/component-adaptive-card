# component-adaptive-card

Render, validate, and (optionally) attach interaction placeholders to **Microsoft Adaptive Cards** — with a focus on **cross-channel compatibility** (Teams, Web Chat, Webex up to 1.3) and **graceful downsampling** when a channel can’t render a full card.

This component is designed to be used inside Greentic flows (via `greentic-dev flow add-step`) and tested locally (via `greentic-component test`).

---

## Component config vs operation input

The component now uses a single canonical component-config surface for install/setup defaults:

- `default_source`
- `default_card_inline`
- `default_card_asset`
- `catalog_registry_ref`
- `multilingual`
- `language_mode`
- `supported_locales`
- `direction_mode`
- `validation_mode`
- `trace_enabled`
- `trace_capture_inputs`

Operation input stays focused on per-call overrides:

- `card_source`
- `card_spec`
- `locale`
- `payload`
- `session`
- `state`
- `interaction`
- `mode`
- `validation_mode`
- `node_id`
- `envelope`

Runtime precedence is:

1. explicit operation input override
2. component config
3. deprecated env-var fallback
4. hardcoded default

English (`en`) is always the implicit baseline and fallback locale.

## Multilingual and direction behavior

- `multilingual = false` forces English rendering behavior
- `multilingual = true` with `language_mode = all` allows the built-in locale set from `config/supported_locales.json`
- `multilingual = true` with `language_mode = custom` uses `supported_locales`, with English still retained as fallback
- `direction_mode = ltr` emits `"rtl": false`
- `direction_mode = rtl` emits `"rtl": true`
- `direction_mode = auto` infers direction from locale and currently treats Arabic variants as RTL

At render time the component injects Adaptive Card root `lang` and `rtl`.

## Catalog registry refs

Use `catalog_registry_ref` for distributable catalog sources, for example:

- `store://greentic-biz/_/adaptive-cards/default`
- `repo://my-repo/cards/catalog.json`

What is implemented today:

- `repo://...` refs are resolved as local repo-relative paths in native/local execution
- `store://...` refs require host/distributor resolution and can be satisfied through the existing host asset resolver hook
- legacy `ADAPTIVE_CARD_CATALOG_FILE` and `ADAPTIVE_CARD_ASSET_REGISTRY` env vars remain as deprecated fallback only

The component does not perform remote distributor fetches by itself inside the wasm runtime.

---

## Greentic ABI compatibility

- Component ABI: `greentic:component/component@0.6.0`
- Guest bindings: `greentic-interfaces-guest` with `features = ["component-v0-6"]`

If you compile this component with different interface versions or features, exports will not match the manifest world.

---

## What are Adaptive Cards?

Adaptive Cards are a JSON-based UI description format (“cards”) that host apps (e.g. Microsoft Teams, Bot Framework Web Chat, Webex) can render. You author a single card payload, and host apps render it according to their capabilities.

Key ideas:
- A card has a top-level schema version (`"version": "1.3"` etc.)
- Hosts support **different subsets** of elements/properties
- The safest approach for broad compatibility is to target **1.3** and avoid features not supported by your host(s)

Official references:
- Adaptive Cards documentation and schema explorer: https://adaptivecards.io/explorer/
- Adaptive Card Designer (interactive authoring): https://adaptivecards.io/designer/

---

## Host support overview

### Microsoft Teams
Teams supports Adaptive Cards across bots, message extensions, and task modules. Actual feature support depends on the client and surface.

Recommendation: Target **Adaptive Cards 1.3** unless you control all clients.

Docs:
https://learn.microsoft.com/microsoftteams/platform/task-modules-and-cards/cards/cards-reference

### Bot Framework Web Chat
Web Chat renders Adaptive Cards using the bundled Adaptive Cards renderer. Supported schema depends on the version shipped with Web Chat.

Recommendation: Treat **1.3** as the safe baseline unless you pin versions end-to-end.

Docs:
https://www.npmjs.com/package/botframework-webchat

### Webex
Webex bots explicitly support **Adaptive Cards 1.3** with a documented subset of elements and properties.

Recommendation: Always target **1.3** for Webex bots.

Docs:
https://developer.webex.com/messaging/docs/buttons-and-cards
https://developer.webex.com/blog/webex-bots-support-for-buttons-and-cards-v1-3

---

## Greentic downsampling behavior

Greentic messaging providers handle cards according to channel capability:

1. **Full Adaptive Card support**
   - Card JSON is delivered as-is (or minimally transformed)

2. **Partial / 1.3-only support**
   - Unsupported elements or properties are stripped or rewritten
   - Schema version may be clamped to `1.3`

3. **No Adaptive Card support**
   - Card is downsampled to readable text/markdown
   - Actions are rendered as textual options or links
   - Inputs are summarized as expected fields

The goal is to preserve **intent**, even when rich UI is unavailable.

---

## Using the component in Greentic flows

### Add step (default mode)

Default mode now centers on component config, not invocation defaults.

```bash
greentic-dev flow add-step \
  --flow flows/main.ygtc \
  --after start \
  --node-id adaptive-card \
  --operation card \
  --payload '{"card_source":"asset","card_spec":{"asset_path":"card.json","template_params":{}},"locale":"en","mode":"renderAndValidate"}' \
  --component oci://ghcr.io/greentic-ai/components/component-adaptive-card:latest
```

Default mode expects an explicit payload, so no interactive prompts are required.

---

### Add step (config mode)

Config mode exposes the full configuration surface.

```bash
greentic-dev flow add-step \
  --flow flows/main.ygtc \
  --after start \
  --node-id adaptive-card \
  --mode config \
  --component oci://ghcr.io/greentic-ai/components/component-adaptive-card:latest \
  --manifest component.manifest.json
```

Or with a custom config flow file:

```bash
greentic-dev flow add-step \
  --flow flows/main.ygtc \
  --after start \
  --node-id adaptive-card \
  --mode config \
  --config-flow ./dev_flows.custom \
  --component oci://ghcr.io/greentic-ai/components/component-adaptive-card:latest \
  --manifest component.manifest.json
```

Config mode now drives the canonical component config:
- selecting default source (`inline`, `asset`, `catalog`)
- default inline card or asset path
- catalog registry refs such as `store://greentic-biz/_/adaptive-cards/default`
- multilingual support
- custom locales such as `en,en-GB,fr,de,nl`
- text direction (`ltr`, `rtl`, `auto`)
- validation and tracing defaults

---

## Conditional prompts with `show_if`

The config flow supports conditional questions via `show_if`. A question is only asked (and required) when its `show_if` evaluates to true against the current answers, in order. If the controlling answer hasn't been asked yet, the dependent question is skipped.

Boolean form (always show or always hide):

```json
{ "id": "debug", "type": "bool", "show_if": true }
{ "id": "internal_only", "type": "string", "show_if": false }
```

Conditional on another answer (string/bool/number/choice):

```json
{ "id": "mode", "type": "choice", "options": ["asset", "inline"] }
{ "id": "asset_path", "type": "string", "required": true,
  "show_if": { "id": "mode", "equals": "asset" } }
```

Notes:
- `show_if` is evaluated against answers collected so far; missing answers mean "hide".
- Only `equals` is supported right now; anything else falls back to "show".

---

## Fetching the component

```bash
greentic-component store fetch \
  --out . \
  oci://ghcr.io/greentic-ai/components/component-adaptive-card:latest
```

This downloads the component artifact locally.

---

## Local testing with greentic-component test

Example test with two sequential steps and state dump:

```bash
greentic-component test \
  --wasm ./component_adaptive_card.wasm \
  --manifest ./component.manifest.json \
  --op card --input-json '{
    "card_source": "asset",
    "card_spec": {
      "asset_path": "card.json",
      "template_params": {}
    },
    "mode": "renderAndValidate",
    "interaction": {
      "enabled": true,
      "interaction_type": "Submit",
      "action_id": "save",
      "card_instance_id": "card-1",
      "raw_inputs": { "comment": "Hello from state" }
    }
  }' \
  --step --op card --input-json '{
    "card_source": "asset",
    "card_spec": {
      "asset_path": "card2.json",
      "template_params": {}
    },
    "mode": "renderAndValidate",
    "interaction": {
      "enabled": true,
      "interaction_type": "Submit",
      "action_id": "save",
      "card_instance_id": "card-1",
      "raw_inputs": {}
    }
  }' \
  --state-dump \
  --pretty
```

---

## Authoring tips

- Use the **Adaptive Card Designer** to validate schema version and supported elements
- Prefer schema **1.3** for cross-channel compatibility
- Avoid host-specific features unless you target a single channel

---

## Troubleshooting

### Card does not render
- Verify schema version compatibility
- Remove unsupported elements for the target host
- Run with `mode=validate` and inspect validation output

### Actions do not trigger
- Confirm correct action type (`Submit` vs `Execute`)
- Verify provider routing and permissions
- Check host limitations on interactive actions

---

## References

- Adaptive Cards Explorer: https://adaptivecards.io/explorer/
- Adaptive Card Designer: https://adaptivecards.io/designer/
- Microsoft Teams Cards: https://learn.microsoft.com/microsoftteams/platform/task-modules-and-cards/cards/cards-reference
- Webex Adaptive Cards: https://developer.webex.com/messaging/docs/buttons-and-cards
