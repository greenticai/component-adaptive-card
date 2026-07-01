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

fn zain_throughput_payload(outcome: &str) -> serde_json::Value {
    json!({
        "query_type": "throughput",
        "outcome": outcome,
        "summary": {
            "template_id": format!("throughput_by_prefix_{outcome}"),
            "fields": {
                "prefix": "203.0.113.0/24",
                "direction": "inbound",
                "time_range": "last day",
                "total_avg_gbps": 1.65,
                "total_p95_gbps": 1.66,
                "total_peak_gbps": 1.66,
                "top_peer": "Peer AS64501",
                "top_peer_pct": 55.4,
                "covering_prefix": "203.0.112.0/23"
            }
        },
        "table": {
            "columns": ["Peer", "Router", "Interface", "Total GB", "Avg bps", "p95 bps", "Peak bps", "% of Top 25"],
            "rows": [["Peer AS64501", "IGW-C1", "Te0/0/0/1", "744.0", "1650000000", "1660000000", "1660000000", "55.4"]],
            "sort_column": "total_bits",
            "sort_direction": "desc"
        },
        "anomalies": [{
            "type": "single_path_dependency",
            "severity": "warning",
            "detail": "All inbound traffic for 203.0.113.0/24 is arriving via a single path"
        }],
        "data_quality": [{
            "type": "sparse_time_series",
            "detail": "Coverage 74.3% (214 of 288 expected intervals) for peer Peer AS64501"
        }],
        "confidence": [{
            "type": "fixed_sampling_caveat",
            "field": "total_bits",
            "detail": "Sampling rate unavailable from Sightline configuration endpoint"
        }],
        "time_series_ref": "sim://flows/203.0.113.0_24/inbound",
        "meta": {
            "queried_prefix": "203.0.113.0/24",
            "effective_prefix": "203.0.113.0/24",
            "direction": "inbound",
            "sample_interval_seconds": 300,
            "interval_count_expected": 288,
            "source": "zain-telco-x simulator via MCP /run_template"
        }
    })
}

fn zain_summary_table_card_template() -> serde_json::Value {
    json!({
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            {
                "type": "Container",
                "style": "emphasis",
                "items": [
                    { "type": "TextBlock", "weight": "Bolder", "text": "Zain inbound throughput" },
                    { "type": "TextBlock", "wrap": true, "text": "{{payload.summary.fields.direction}} throughput for {{payload.summary.fields.prefix}}: {{payload.summary.fields.total_p95_gbps}} Gbps p95; top peer {{payload.summary.fields.top_peer}}" },
                    { "type": "FactSet", "facts": [
                        { "title": "Outcome", "value": "{{payload.outcome}}" },
                        { "title": "Prefix", "value": "{{payload.summary.fields.prefix}}" },
                        { "title": "Direction", "value": "{{payload.summary.fields.direction}}" },
                        { "title": "p95", "value": "{{payload.summary.fields.total_p95_gbps}} Gbps" }
                    ]}
                ]
            },
            {
                "type": "Table",
                "columns": [
                    { "width": 2 }, { "width": 1 }, { "width": 1 }, { "width": 1 },
                    { "width": 1 }, { "width": 1 }, { "width": 1 }, { "width": 1 }
                ],
                "rows": [
                    { "type": "TableRow", "style": "accent", "cells": [
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "weight": "Bolder", "wrap": true, "text": "Peer" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "weight": "Bolder", "wrap": true, "text": "Router" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "weight": "Bolder", "wrap": true, "text": "Interface" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "weight": "Bolder", "wrap": true, "text": "Total GB" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "weight": "Bolder", "wrap": true, "text": "Avg bps" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "weight": "Bolder", "wrap": true, "text": "p95 bps" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "weight": "Bolder", "wrap": true, "text": "Peak bps" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "weight": "Bolder", "wrap": true, "text": "% of Top 25" }] }
                    ]},
                    { "type": "TableRow", "cells": [
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "wrap": true, "text": "{{payload.table.rows.[0].[0]}}" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "wrap": true, "text": "{{payload.table.rows.[0].[1]}}" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "wrap": true, "text": "{{payload.table.rows.[0].[2]}}" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "wrap": true, "text": "{{payload.table.rows.[0].[3]}}" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "wrap": true, "text": "{{payload.table.rows.[0].[4]}}" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "wrap": true, "text": "{{payload.table.rows.[0].[5]}}" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "wrap": true, "text": "{{payload.table.rows.[0].[6]}}" }] },
                        { "type": "TableCell", "items": [{ "type": "TextBlock", "wrap": true, "text": "{{payload.table.rows.[0].[7]}}" }] }
                    ]}
                ]
            },
            {
                "type": "Container",
                "style": "warning",
                "items": [
                    { "type": "TextBlock", "weight": "Bolder", "text": "Warning: {{payload.anomalies.[0].type}}" },
                    { "type": "TextBlock", "wrap": true, "text": "{{payload.anomalies.[0].detail}}" }
                ]
            },
            {
                "type": "Container",
                "style": "attention",
                "items": [
                    { "type": "TextBlock", "weight": "Bolder", "text": "Data quality: {{payload.data_quality.[0].type}}" },
                    { "type": "TextBlock", "wrap": true, "text": "{{payload.data_quality.[0].detail}}" }
                ]
            },
            {
                "type": "Container",
                "style": "emphasis",
                "items": [
                    { "type": "TextBlock", "weight": "Bolder", "text": "Confidence" },
                    { "type": "TextBlock", "wrap": true, "text": "{{payload.confidence.[0].type}}: {{payload.confidence.[0].detail}}" }
                ]
            },
            {
                "type": "FactSet",
                "facts": [
                    { "title": "Evidence", "value": "{{payload.time_series_ref}}" },
                    { "title": "Source", "value": "{{payload.meta.source}}" },
                    { "title": "Sample interval", "value": "{{payload.meta.sample_interval_seconds}}s" }
                ]
            }
        ],
        "actions": [
            { "type": "Action.Submit", "title": "Show outbound throughput for {{payload.summary.fields.prefix}}, last day", "data": { "text": "Show outbound throughput for {{payload.summary.fields.prefix}}, last day" } },
            { "type": "Action.Submit", "title": "Which IGWs are advertising {{payload.summary.fields.prefix}}?", "data": { "text": "Which IGWs are advertising {{payload.summary.fields.prefix}}?" } }
        ]
    })
}

fn zain_exception_card_template() -> serde_json::Value {
    json!({
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "TextBlock", "weight": "Bolder", "text": "Throughput request requires attention" },
            { "type": "FactSet", "facts": [
                { "title": "Issue", "value": "{{payload.outcome}}" },
                { "title": "Prefix", "value": "{{payload.summary.fields.prefix}}" },
                { "title": "Resolution", "value": "Check the prefix or query a covering aggregate if appropriate." }
            ]},
            { "type": "TextBlock", "wrap": true, "text": "Effective prefix: {{payload.meta.effective_prefix}}" }
        ]
    })
}

fn zain_clarification_payload() -> serde_json::Value {
    json!({
        "query_type": "throughput",
        "outcome": "clarification",
        "summary": {
            "template_id": "throughput_missing_prefix",
            "fields": {
                "missing_parameter": "prefix",
                "guidance": "Please include a prefix, for example 203.0.113.0/24."
            }
        }
    })
}

fn zain_clarification_card_template() -> serde_json::Value {
    json!({
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            { "type": "TextBlock", "weight": "Bolder", "text": "Clarification required" },
            { "type": "FactSet", "facts": [
                { "title": "Missing parameter", "value": "{{payload.summary.fields.missing_parameter}}" },
                { "title": "Guidance", "value": "{{payload.summary.fields.guidance}}" }
            ]}
        ]
    })
}

fn render_zain_card(template: serde_json::Value, payload: serde_json::Value) -> serde_json::Value {
    let mut invocation = base_invocation(template);
    invocation.payload = payload;
    invocation.validation_mode = ValidationMode::Error;
    let result = handle_invocation(invocation).expect("Zain card should render and validate");
    assert!(
        result.validation_issues.is_empty(),
        "unexpected validation issues: {:?}",
        result.validation_issues
    );
    result.rendered_card.expect("card should render")
}

fn rendered_texts(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("type").and_then(serde_json::Value::as_str) == Some("TextBlock") {
                if let Some(text) = map.get("text").and_then(serde_json::Value::as_str) {
                    out.push(text.to_string());
                }
            }
            if let Some(title) = map.get("title").and_then(serde_json::Value::as_str) {
                out.push(title.to_string());
            }
            if let Some(value) = map.get("value").and_then(serde_json::Value::as_str) {
                out.push(value.to_string());
            }
            for nested in map.values() {
                rendered_texts(nested, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                rendered_texts(item, out);
            }
        }
        _ => {}
    }
}

fn rendered_contains(card: &serde_json::Value, needle: &str) -> bool {
    let mut texts = Vec::new();
    rendered_texts(card, &mut texts);
    texts.iter().any(|text| text.contains(needle))
}

#[test]
fn zain_throughput_summary_table_card_renders_payload_contract() {
    let card = render_zain_card(
        zain_summary_table_card_template(),
        zain_throughput_payload("normal"),
    );
    assert_eq!(
        card["$schema"],
        "http://adaptivecards.io/schemas/adaptive-card.json"
    );
    assert_eq!(card["type"], "AdaptiveCard");
    assert_eq!(card["version"], "1.6");
    assert!(rendered_contains(&card, "Zain inbound throughput"));
    assert!(rendered_contains(&card, "203.0.113.0/24"));
    assert!(rendered_contains(&card, "Peer AS64501"));
    assert!(rendered_contains(&card, "Total GB"));
    assert!(rendered_contains(&card, "p95 bps"));
    assert!(rendered_contains(
        &card,
        "sim://flows/203.0.113.0_24/inbound"
    ));
    assert!(rendered_contains(
        &card,
        "Show outbound throughput for 203.0.113.0/24, last day"
    ));
}

#[test]
fn zain_throughput_anomaly_data_quality_and_confidence_panels_are_distinct() {
    let card = render_zain_card(
        zain_summary_table_card_template(),
        zain_throughput_payload("single_path_dependency"),
    );
    assert!(card.to_string().contains("\"style\":\"warning\""));
    assert!(card.to_string().contains("\"style\":\"attention\""));
    assert!(rendered_contains(&card, "Warning: single_path_dependency"));
    assert!(rendered_contains(&card, "Data quality: sparse_time_series"));
    assert!(rendered_contains(&card, "fixed_sampling_caveat"));
}

#[test]
fn zain_throughput_exception_outcomes_render_as_adaptive_card_1_6() {
    for outcome in ["prefix_not_found", "covering_aggregate"] {
        let mut payload = zain_throughput_payload(outcome);
        payload["meta"]["effective_prefix"] = if outcome == "covering_aggregate" {
            json!("203.0.112.0/23")
        } else {
            json!("203.0.113.0/24")
        };
        let card = render_zain_card(zain_exception_card_template(), payload);
        assert_eq!(card["version"], "1.6");
        assert!(rendered_contains(&card, outcome));
        assert!(rendered_contains(&card, "Resolution"));
    }
}

#[test]
fn zain_throughput_clarification_card_renders_missing_prefix_guidance() {
    let card = render_zain_card(
        zain_clarification_card_template(),
        zain_clarification_payload(),
    );
    assert_eq!(card["version"], "1.6");
    assert!(rendered_contains(&card, "Clarification required"));
    assert!(rendered_contains(&card, "prefix"));
    assert!(rendered_contains(&card, "Please include a prefix"));
}
