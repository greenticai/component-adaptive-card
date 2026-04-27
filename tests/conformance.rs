use component_adaptive_card::{
    AdaptiveCardInvocation, CanonicalInvocationEnvelope, CardInteraction, CardInteractionType,
    CardSource, CardSpec, InvocationMode, ValidationMode, handle_invocation,
    register_host_asset_callback,
};
use serde_json::json;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;

fn manifest_json() -> serde_json::Value {
    serde_json::from_str(include_str!("../component.manifest.json"))
        .expect("component manifest should be valid json")
}

fn base_invocation(card: serde_json::Value) -> AdaptiveCardInvocation {
    AdaptiveCardInvocation {
        card_source: CardSource::Inline,
        card_spec: CardSpec {
            inline_json: Some(card),
            asset_path: None,
            catalog_name: None,
            template_params: None,
            asset_registry: None,
            i18n_bundle_path: None,
            i18n_inline: None,
        },
        node_id: None,
        locale: None,
        payload: json!({}),
        session: json!({}),
        state: json!({}),
        interaction: None,
        mode: InvocationMode::RenderAndValidate,
        validation_mode: ValidationMode::Warn,
        envelope: None,
    }
}

fn envelope_with_locale(locale: &str) -> CanonicalInvocationEnvelope {
    CanonicalInvocationEnvelope {
        ctx: greentic_interfaces_guest::component_v0_6::node::TenantCtx {
            tenant_id: "tenant".to_string(),
            team_id: None,
            user_id: None,
            env_id: "dev".to_string(),
            trace_id: "trace".to_string(),
            correlation_id: "corr".to_string(),
            deadline_ms: 0,
            attempt: 1,
            idempotency_key: None,
            i18n_id: locale.to_string(),
        },
        flow_id: "flow".to_string(),
        step_id: "step".to_string(),
        component_id: "ai.greentic.component-adaptive-card".to_string(),
        attempt: 1,
        payload_cbor: Vec::new(),
        metadata_cbor: None,
    }
}

#[test]
fn describe_mentions_world() {
    let payload = component_adaptive_card::describe_payload();
    let json: serde_json::Value = serde_json::from_str(&payload).expect("describe should be json");
    assert_eq!(
        json["component"]["world"],
        "greentic:component/component@0.6.0"
    );
}

#[test]
fn manifest_dev_flows_use_conditional_questions() {
    let manifest = manifest_json();
    let default_fields =
        manifest["dev_flows"]["default"]["graph"]["nodes"]["ask_config"]["questions"]["fields"]
            .as_array()
            .expect("default flow fields");
    let custom_fields =
        manifest["dev_flows"]["custom"]["graph"]["nodes"]["ask_config"]["questions"]["fields"]
            .as_array()
            .expect("custom flow fields");

    let inline_default = default_fields
        .iter()
        .find(|field| field["id"] == "default_card_inline")
        .expect("default inline field");
    assert_eq!(inline_default["show_if"]["id"], "default_source");
    assert_eq!(inline_default["show_if"]["equals"], "inline");

    let asset_default = default_fields
        .iter()
        .find(|field| field["id"] == "default_card_asset")
        .expect("default asset field");
    assert_eq!(asset_default["show_if"]["equals"], "asset");

    let remote_default = default_fields
        .iter()
        .find(|field| field["id"] == "default_card_remote")
        .expect("default remote field");
    assert_eq!(remote_default["show_if"]["equals"], "remote");

    let language_mode_default = default_fields
        .iter()
        .find(|field| field["id"] == "language_mode")
        .expect("default language mode field");
    assert_eq!(language_mode_default["show_if"]["id"], "multilingual");
    assert_eq!(language_mode_default["show_if"]["equals"], true);

    let locales_default = default_fields
        .iter()
        .find(|field| field["id"] == "supported_locales")
        .expect("default locales field");
    assert_eq!(locales_default["show_if"]["id"], "language_mode");
    assert_eq!(locales_default["show_if"]["equals"], "custom");

    let trace_capture_custom = custom_fields
        .iter()
        .find(|field| field["id"] == "trace_capture_inputs")
        .expect("custom trace capture field");
    assert_eq!(trace_capture_custom["show_if"]["id"], "trace_enabled");
    assert_eq!(trace_capture_custom["show_if"]["equals"], true);
}

#[test]
fn inline_render_returns_card_and_features() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "TextBlock", "text": "Hello" }
        ]
    });
    let invocation = base_invocation(card.clone());
    let result = handle_invocation(invocation).expect("render should succeed");
    let rendered = result.rendered_card.expect("card should render");
    assert_eq!(rendered["type"], card["type"]);
    assert_eq!(rendered["version"], card["version"]);
    assert_eq!(rendered["body"], card["body"]);
    assert_eq!(rendered["lang"], "en");
    assert_eq!(rendered["rtl"], false);
    assert!(
        result
            .card_features
            .used_elements
            .contains(&"TextBlock".to_string())
    );
}

#[test]
fn parses_runner_payload_wrapper() {
    let input = serde_json::json!({
        "context": {
            "team_id": "default",
            "tenant_id": "local-dev",
            "user_id": "developer"
        },
        "payload": {
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
            "payload": { "user": { "name": "Ada" } }
        }
    });
    let input_str = serde_json::to_string(&input).unwrap();
    let output = component_adaptive_card::handle_message("card", &input_str);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(
        parsed.get("error").is_none(),
        "unexpected error payload: {parsed}"
    );
}

#[test]
fn handlebars_renders_payload_and_state_input() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "TextBlock", "text": "Hello {{payload.user.name}}" },
            { "type": "TextBlock", "text": "Input: {{name}}" }
        ]
    });
    let mut invocation = base_invocation(card.clone());
    invocation.payload = json!({ "user": { "name": "Ada" } });
    invocation.state = json!({ "input": { "name": "ImplicitAda" } });

    let result = handle_invocation(invocation).expect("render should succeed");
    let rendered = result.rendered_card.expect("card should render");
    assert_eq!(rendered["body"][0]["text"], "Hello Ada");
    assert_eq!(rendered["body"][1]["text"], "Input: ImplicitAda");
}

#[test]
fn asset_render_loads_card() {
    let spec = CardSpec {
        asset_path: Some("tests/assets/cards/simple.json".to_string()),
        ..Default::default()
    };
    let invocation = AdaptiveCardInvocation {
        card_source: CardSource::Asset,
        card_spec: spec,
        node_id: None,
        locale: None,
        payload: json!({}),
        session: json!({}),
        state: json!({}),
        interaction: None,
        mode: InvocationMode::RenderAndValidate,
        validation_mode: ValidationMode::Warn,
        envelope: None,
    };

    let result = handle_invocation(invocation).expect("asset render");
    let card = result.rendered_card.expect("card should render");
    assert_eq!(card["type"], "AdaptiveCard");
    assert!(
        result
            .card_features
            .used_elements
            .contains(&"TextBlock".to_string())
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn catalog_resolution_uses_env_mapping() {
    let mapping = json!({ "sample": "tests/assets/cards/simple.json" });
    let catalog_file = std::env::temp_dir().join("adaptive_card_catalog_test.json");
    fs::write(&catalog_file, serde_json::to_string(&mapping).unwrap()).unwrap();
    unsafe {
        std::env::set_var(
            "ADAPTIVE_CARD_CATALOG_FILE",
            catalog_file.to_string_lossy().to_string(),
        );
    }

    let invocation = AdaptiveCardInvocation {
        card_source: CardSource::Catalog,
        card_spec: CardSpec {
            catalog_name: Some("sample".to_string()),
            asset_registry: None,
            ..Default::default()
        },
        node_id: None,
        locale: None,
        payload: json!({}),
        session: json!({}),
        state: json!({}),
        interaction: None,
        mode: InvocationMode::RenderAndValidate,
        validation_mode: ValidationMode::Warn,
        envelope: None,
    };

    let result = handle_invocation(invocation).expect("catalog render");
    let card = result.rendered_card.expect("card should render");
    assert_eq!(card["type"], "AdaptiveCard");
}

#[test]
fn bindings_apply_session_and_state() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "TextBlock", "text": "Hello @{session.user.name}, step ${state.step}" }
        ]
    });
    let mut invocation = base_invocation(card);
    invocation.session = json!({ "user": { "name": "Ada" }});
    invocation.state = json!({ "step": 2 });

    let result = handle_invocation(invocation).expect("render with bindings");
    let rendered = result.rendered_card.expect("card should render");
    let text = rendered["body"][0]["text"]
        .as_str()
        .expect("text should be string");
    assert_eq!(text, "Hello Ada, step 2");
}

#[test]
fn bindings_apply_default_with_coalesce() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "TextBlock", "text": "Hello @{session.user.name||\"Guest\"}" }
        ]
    });
    let invocation = base_invocation(card);
    let result = handle_invocation(invocation).expect("render with default");
    let rendered = result.rendered_card.expect("card should render");
    let text = rendered["body"][0]["text"]
        .as_str()
        .expect("text should be string");
    assert_eq!(text, "Hello Guest");
}

#[test]
fn expression_placeholders_support_equality_and_ternary() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "TextBlock", "text": "${payload.status == \"ok\" ? \"green\" : \"red\"}" }
        ]
    });
    let mut invocation = base_invocation(card);
    invocation.payload = json!({ "status": "ok" });
    let result = handle_invocation(invocation).expect("expression render");
    let rendered = result.rendered_card.expect("card should render");
    let text = rendered["body"][0]["text"]
        .as_str()
        .expect("text should be string");
    assert_eq!(text, "green");
}

#[test]
fn submit_interaction_emits_event_and_updates_state() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "Input.Text", "id": "comment" }
        ]
    });
    let mut invocation = base_invocation(card);
    invocation.interaction = Some(CardInteraction {
        enabled: None,
        interaction_type: CardInteractionType::Submit,
        action_id: "submit-1".to_string(),
        verb: None,
        raw_inputs: json!({ "comment": "Looks good" }),
        card_instance_id: "card-1".to_string(),
        metadata: json!({ "route": "next" }),
    });

    let result = handle_invocation(invocation).expect("interaction");
    let event = result.event.expect("event should exist");
    assert_eq!(event.action_id, "submit-1");
    assert_eq!(event.inputs["comment"], "Looks good");

    assert!(result
        .state_updates
        .iter()
        .any(|op| matches!(op, component_adaptive_card::StateUpdateOp::Merge { path, .. } if path == "form_data")));
    assert!(result
        .session_updates
        .iter()
        .any(|op| matches!(op, component_adaptive_card::SessionUpdateOp::SetRoute { route } if route == "next")));
}

#[test]
fn toggle_visibility_sets_state_flag() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "actions": [
            { "type": "Action.ToggleVisibility", "targetElements": ["section-1"] }
        ]
    });
    let mut invocation = base_invocation(card);
    invocation.interaction = Some(CardInteraction {
        enabled: None,
        interaction_type: CardInteractionType::ToggleVisibility,
        action_id: "section-1".to_string(),
        verb: None,
        raw_inputs: json!({}),
        card_instance_id: "card-2".to_string(),
        metadata: json!({ "visible": false }),
    });

    let result = handle_invocation(invocation).expect("toggle");
    assert!(result
        .state_updates
        .iter()
        .any(|op| matches!(op, component_adaptive_card::StateUpdateOp::Set { path, value } if path == "ui.visibility.section-1" && value == &json!(false))));
}

#[test]
fn feature_summary_detects_actions_and_media() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "Media", "sources": [ { "mimeType": "video/mp4", "url": "https://example.com" } ] }
        ],
        "actions": [
            { "type": "Action.ShowCard", "card": { "type": "AdaptiveCard", "version": "1.6", "body": [] } },
            { "type": "Action.ToggleVisibility", "targetElements": ["x"] }
        ]
    });
    let invocation = base_invocation(card);
    let result = handle_invocation(invocation).expect("feature detection");

    assert!(result.card_features.uses_media);
    assert!(result.card_features.uses_show_card);
    assert!(result.card_features.uses_toggle_visibility);
    assert!(
        result
            .card_features
            .used_actions
            .iter()
            .any(|a| a == "Action.ShowCard")
    );
}

#[test]
fn validation_reports_choice_set_and_toggle_rules() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "Input.ChoiceSet", "id": "choices" },
            { "type": "Input.Toggle", "id": "toggle", "title": "" }
        ],
        "actions": [
            { "type": "Action.ToggleVisibility", "targetElements": [] },
            { "type": "Action.ShowCard", "card": "invalid" }
        ]
    });
    let invocation = base_invocation(card);
    let result = handle_invocation(invocation).expect("validation");
    let issues: Vec<String> = result
        .validation_issues
        .iter()
        .map(|v| v.code.clone())
        .collect();
    assert!(issues.iter().any(|c| c == "missing-choices"));
    assert!(issues.iter().any(|c| c == "missing-title"));
    assert!(issues.iter().any(|c| c == "empty-target-elements"));
    assert!(issues.iter().any(|c| c == "invalid-card"));
}

#[test]
fn validation_catches_media_sources() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "Media", "sources": [] }
        ]
    });
    let invocation = base_invocation(card);
    let result = handle_invocation(invocation).expect("validation");
    let codes: Vec<String> = result
        .validation_issues
        .iter()
        .map(|i| i.code.clone())
        .collect();
    assert!(codes.iter().any(|c| c == "missing-sources"));
}

#[test]
fn host_asset_registry_resolves_assets() {
    let _ = register_host_asset_callback(Box::new(|name| {
        if name == "host-card" {
            Some("tests/assets/cards/simple.json".to_string())
        } else {
            None
        }
    }));
    let invocation = AdaptiveCardInvocation {
        card_source: CardSource::Asset,
        card_spec: CardSpec {
            asset_path: Some("host-card".to_string()),
            ..Default::default()
        },
        node_id: None,
        locale: None,
        payload: json!({}),
        session: json!({}),
        state: json!({}),
        interaction: None,
        mode: InvocationMode::RenderAndValidate,
        validation_mode: ValidationMode::Warn,
        envelope: None,
    };

    let result = handle_invocation(invocation).expect("host registry");
    let card = result.rendered_card.expect("card should render");
    assert_eq!(card["type"], "AdaptiveCard");
}

#[test]
fn i18n_marker_prefers_invocation_locale_over_session_and_envelope() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "actions": [
            { "type": "Action.Submit", "title": "{{i18n:card.action.save}}", "id": "save" }
        ]
    });
    let mut invocation = base_invocation(card);
    invocation.locale = Some("en-GB".to_string());
    invocation.session = json!({ "locale": "ar" });
    invocation.envelope = Some(envelope_with_locale("ar"));

    let result = handle_invocation(invocation).expect("render should succeed");
    let rendered = result.rendered_card.expect("card should render");
    assert_eq!(rendered["actions"][0]["title"], "Save (UK)");
}

#[test]
fn i18n_marker_uses_session_locale_when_invocation_locale_missing() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "actions": [
            { "type": "Action.Submit", "title": "{{i18n:card.action.save}}", "id": "save" }
        ]
    });
    let mut invocation = base_invocation(card);
    invocation.session = json!({ "locale": "ar" });

    let result = handle_invocation(invocation).expect("render should succeed");
    let rendered = result.rendered_card.expect("card should render");
    assert_eq!(rendered["actions"][0]["title"], "حفظ");
}

#[test]
fn i18n_marker_uses_envelope_locale_when_others_missing() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "actions": [
            { "type": "Action.Submit", "title": "{{i18n:card.action.save}}", "id": "save" }
        ]
    });
    let mut invocation = base_invocation(card);
    invocation.envelope = Some(envelope_with_locale("en-GB"));

    let result = handle_invocation(invocation).expect("render should succeed");
    let rendered = result.rendered_card.expect("card should render");
    assert_eq!(rendered["actions"][0]["title"], "Save (UK)");
}

#[test]
fn official_locale_field_sets_root_lang() {
    let card = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [{ "type": "TextBlock", "text": "Hello" }]
    });
    let mut invocation = base_invocation(card);
    invocation.locale = Some("fr".to_string());

    let result = handle_invocation(invocation).expect("render should succeed");
    let rendered = result.rendered_card.expect("card should render");
    assert_eq!(rendered["lang"], "fr");
    assert_eq!(rendered["rtl"], false);
}

#[test]
fn deprecated_i18n_locale_alias_is_still_accepted() {
    let input = serde_json::json!({
        "i18n_locale": "ar",
        "card_source": "inline",
        "card_spec": {
            "inline_json": {
                "type": "AdaptiveCard",
                "version": "1.6",
                "body": [{ "type": "TextBlock", "text": "Hello" }]
            }
        }
    });
    let output = component_adaptive_card::handle_message("card", &input.to_string());
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("render output");
    assert_eq!(parsed["renderedCard"]["lang"], "ar");
}

#[test]
fn config_defaults_supply_inline_card_at_runtime() {
    let input = serde_json::json!({
        "config": {
            "default_source": "inline",
            "default_card_inline": {
                "type": "AdaptiveCard",
                "version": "1.6",
                "body": [{ "type": "TextBlock", "text": "Configured default" }]
            }
        },
        "card_spec": {},
        "payload": { "user": { "name": "Ada" } }
    });
    let output = component_adaptive_card::handle_message("card", &input.to_string());
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("render output");
    assert_eq!(
        parsed["renderedCard"]["body"][0]["text"],
        "Configured default"
    );
}

#[test]
fn schema_validation_reports_missing_card_spec() {
    let input = serde_json::json!({
        "card_source": "asset",
        "validation_mode": "error"
    });
    let output = component_adaptive_card::handle_message("card", &input.to_string());
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("schema error payload");
    let issues = parsed["error"]["details"]["validation_issues"]
        .as_array()
        .expect("validation issues array");

    assert_eq!(parsed["error"]["code"], "AC_SCHEMA_INVALID");
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "AC_INVOCATION_MISSING_FIELD")
    );
}

#[test]
fn auto_direction_marks_arabic_locales_as_rtl() {
    let input = serde_json::json!({
        "config": {
            "default_source": "inline",
            "default_card_inline": {
                "type": "AdaptiveCard",
                "version": "1.6",
                "body": [{ "type": "TextBlock", "text": "مرحبا" }]
            },
            "multilingual": true,
            "language_mode": "all",
            "direction_mode": "auto"
        },
        "card_spec": {},
        "locale": "ar-SA"
    });
    let output = component_adaptive_card::handle_message("card", &input.to_string());
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("render output");
    assert_eq!(parsed["renderedCard"]["lang"], "ar-SA");
    assert_eq!(parsed["renderedCard"]["rtl"], true);
}

#[test]
fn custom_locale_mode_filters_unsupported_requested_locale() {
    let input = serde_json::json!({
        "config": {
            "default_source": "inline",
            "default_card_inline": {
                "type": "AdaptiveCard",
                "version": "1.6",
                "body": [{ "type": "TextBlock", "text": "Hello" }]
            },
            "multilingual": true,
            "language_mode": "custom",
            "supported_locales": ["en", "fr"],
            "direction_mode": "auto"
        },
        "card_spec": {},
        "locale": "de"
    });
    let output = component_adaptive_card::handle_message("card", &input.to_string());
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("render output");
    assert_eq!(parsed["renderedCard"]["lang"], "en");
    assert_eq!(parsed["renderedCard"]["rtl"], false);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn repo_catalog_registry_ref_resolves_catalog_mapping() {
    let temp_root = std::env::temp_dir().join("adaptive-card-repo-ref-test");
    fs::create_dir_all(temp_root.join("cards")).unwrap();
    let original = std::env::current_dir().unwrap();
    let sample_card = original.join("tests/assets/cards/simple.json");
    fs::write(
        temp_root.join("cards/catalog.json"),
        serde_json::to_string(&json!({ "sample": sample_card.to_string_lossy() })).unwrap(),
    )
    .unwrap();
    std::env::set_current_dir(&temp_root).unwrap();

    let input = serde_json::json!({
        "config": {
            "default_source": "catalog",
            "catalog_registry_ref": "repo://my-repo/cards/catalog.json"
        },
        "card_source": "catalog",
        "card_spec": {
            "catalog_name": "sample"
        }
    });
    let output = component_adaptive_card::handle_message("card", &input.to_string());
    std::env::set_current_dir(original).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&output).expect("render output");
    assert_eq!(parsed["renderedCard"]["type"], "AdaptiveCard");
}

#[test]
fn runtime_errors_emit_msg_key_and_localized_message() {
    let input = serde_json::json!({
        "locale": "en-GB",
        "payload": {
            "card_source": "asset",
            "card_spec": {}
        }
    });
    let output = component_adaptive_card::handle_message("card", &input.to_string());
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("error payload");
    assert_eq!(parsed["error"]["msg_key"], "errors.invalid_input");
    assert_eq!(parsed["error"]["message"], "Invalid input (UK)");
}
