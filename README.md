# component-adaptive-card

Render, validate, and (optionally) attach interaction placeholders to **Microsoft Adaptive Cards** — with a focus on **cross-channel compatibility** (Teams, Web Chat, Webex up to 1.3) and **graceful downsampling** when a channel can’t render a full card.

This component is designed to be used inside Greentic flows (via `greentic-dev flow add-step`) and tested locally (via `greentic-component test`).

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

Default mode assumes:
- `card_source = asset`
- You provide `card_spec.asset_path`
- Optional interaction placeholders

```bash
greentic-dev flow add-step \
  --flow flows/main.ygtc \
  --after start \
  --node-id adaptive-card \
  --operation card \
  --payload '{"card_source":"asset","card_spec":{"asset_path":"card.json","template_params":{}},"mode":"renderAndValidate"}' \
  --component oci://ghcr.io/greenticai/components/component-adaptive-card:latest
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
  --component oci://ghcr.io/greenticai/components/component-adaptive-card:latest \
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
  --component oci://ghcr.io/greenticai/components/component-adaptive-card:latest \
  --manifest component.manifest.json
```

Config mode allows:
- Selecting card source (`asset`, `inline`, `catalog`)
- Full card spec configuration
- Template parameters and binding context
- Render / validate behavior
- Interaction placeholders
- Output and debug options

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
  oci://ghcr.io/greenticai/components/component-adaptive-card:latest
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

