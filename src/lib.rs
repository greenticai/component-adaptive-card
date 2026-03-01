#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod asset_resolver;
mod error;
mod expression;
mod i18n;
mod i18n_bundle;
mod interaction;
mod model;
mod render;
mod state_store;
mod trace;
mod validation;

use std::collections::BTreeMap;

#[cfg(target_arch = "wasm32")]
use greentic_interfaces_guest::component_v0_6::node;
use greentic_types::cbor::canonical;
use greentic_types::schemas::common::schema_ir::{AdditionalProperties, SchemaIr};
#[cfg(target_arch = "wasm32")]
use greentic_types::schemas::component::v0_6_0::{
    ChoiceOption, ComponentDescribe, ComponentInfo, ComponentOperation, ComponentQaSpec,
    ComponentRunInput, ComponentRunOutput, I18nText, QaMode as QaModeModel, Question, QuestionKind,
    schema_hash,
};
use once_cell::sync::Lazy;

pub use asset_resolver::{
    register_host_asset_callback, register_host_asset_map, register_host_asset_resolver,
};
pub use error::ComponentError;
pub use interaction::handle_interaction;
pub use model::*;
pub use render::render_card;

const COMPONENT_NAME: &str = "component-adaptive-card";
const COMPONENT_ORG: &str = "ai.greentic";
const COMPONENT_VERSION: &str = env!("CARGO_PKG_VERSION");

static COMPONENT_SCHEMA_JSON: Lazy<serde_json::Value> = Lazy::new(|| {
    serde_json::from_str(include_str!("../schemas/component.schema.json"))
        .expect("failed to parse component schema")
});
static INPUT_SCHEMA_JSON: Lazy<serde_json::Value> = Lazy::new(|| {
    serde_json::from_str(include_str!("../schemas/io/input.schema.json"))
        .expect("failed to parse input schema")
});
static OUTPUT_SCHEMA_JSON: Lazy<serde_json::Value> = Lazy::new(|| {
    serde_json::from_str(include_str!("../schemas/io/output.schema.json"))
        .expect("failed to parse output schema")
});

#[cfg(target_arch = "wasm32")]
#[used]
#[unsafe(link_section = ".greentic.wasi")]
static WASI_TARGET_MARKER: [u8; 13] = *b"wasm32-wasip2";

#[cfg(target_arch = "wasm32")]
mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "component-v0-v6-v0",
    });
}

#[cfg(target_arch = "wasm32")]
use bindings::exports::greentic::component::{
    component_descriptor, component_i18n,
    component_qa::{self, QaMode},
    component_runtime, component_schema,
};

#[cfg(target_arch = "wasm32")]
struct Component;

#[cfg(target_arch = "wasm32")]
struct NodeCompat;

#[cfg(target_arch = "wasm32")]
impl node::Guest for NodeCompat {
    fn describe() -> node::ComponentDescriptor {
        #[cfg(feature = "state-store")]
        let capabilities = vec!["host:state".to_string()];
        #[cfg(not(feature = "state-store"))]
        let capabilities = Vec::new();

        node::ComponentDescriptor {
            name: COMPONENT_NAME.to_string(),
            version: COMPONENT_VERSION.to_string(),
            summary: Some("Adaptive card renderer".to_string()),
            capabilities,
            ops: vec![node::Op {
                name: "card".to_string(),
                summary: Some("Render or validate adaptive cards".to_string()),
                input: node::IoSchema {
                    schema: node::SchemaSource::InlineCbor(input_schema_cbor()),
                    content_type: "application/cbor".to_string(),
                    schema_version: None,
                },
                output: node::IoSchema {
                    schema: node::SchemaSource::InlineCbor(output_schema_cbor()),
                    content_type: "application/cbor".to_string(),
                    schema_version: None,
                },
                examples: Vec::new(),
            }],
            schemas: Vec::new(),
            setup: None,
        }
    }

    fn invoke(
        _op: String,
        envelope: node::InvocationEnvelope,
    ) -> Result<node::InvocationResult, node::NodeError> {
        let (output, _new_state) = run_component_cbor(envelope.payload_cbor, Vec::new());
        Ok(node::InvocationResult {
            ok: true,
            output_cbor: output,
            output_metadata_cbor: None,
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl component_descriptor::Guest for Component {
    fn get_component_info() -> Vec<u8> {
        component_info_cbor()
    }

    fn describe() -> Vec<u8> {
        component_describe_cbor()
    }
}

#[cfg(target_arch = "wasm32")]
impl component_schema::Guest for Component {
    fn input_schema() -> Vec<u8> {
        input_schema_cbor()
    }

    fn output_schema() -> Vec<u8> {
        output_schema_cbor()
    }

    fn config_schema() -> Vec<u8> {
        config_schema_cbor()
    }
}

#[cfg(target_arch = "wasm32")]
impl component_runtime::Guest for Component {
    fn run(input: Vec<u8>, state: Vec<u8>) -> component_runtime::RunResult {
        let (output, new_state) = run_component_cbor(input, state);
        component_runtime::RunResult { output, new_state }
    }
}

#[cfg(target_arch = "wasm32")]
impl component_qa::Guest for Component {
    fn qa_spec(mode: QaMode) -> Vec<u8> {
        let (mode_key, spec_mode) = match mode {
            QaMode::Default => ("default", QaModeModel::Default),
            QaMode::Setup => ("setup", QaModeModel::Setup),
            QaMode::Update => ("update", QaModeModel::Update),
            QaMode::Remove => ("remove", QaModeModel::Remove),
        };
        let questions = qa_card_questions_for_mode(mode_key);
        encode_cbor(&ComponentQaSpec {
            mode: spec_mode,
            title: I18nText::new(format!("qa.{mode_key}.title"), None),
            description: Some(I18nText::new(format!("qa.{mode_key}.description"), None)),
            questions,
            defaults: BTreeMap::new(),
        })
    }

    fn apply_answers(mode: QaMode, current_config: Vec<u8>, answers: Vec<u8>) -> Vec<u8> {
        if matches!(mode, QaMode::Remove) {
            return encode_cbor(&serde_json::json!({}));
        }
        let merged = qa_apply_answers_json(&current_config, &answers);
        encode_cbor(&merged)
    }
}

#[cfg(target_arch = "wasm32")]
impl component_i18n::Guest for Component {
    fn i18n_keys() -> Vec<String> {
        vec![
            "qa.default.title".to_string(),
            "qa.default.description".to_string(),
            "qa.question.card_source.label".to_string(),
            "qa.question.card_source.help".to_string(),
            "qa.question.card_source.option.asset".to_string(),
            "qa.question.card_source.option.inline".to_string(),
            "qa.question.card_source.option.catalog".to_string(),
            "qa.question.card_input.label".to_string(),
            "qa.question.card_input.help".to_string(),
            "qa.setup.title".to_string(),
            "qa.setup.description".to_string(),
            "qa.update.title".to_string(),
            "qa.update.description".to_string(),
            "qa.remove.title".to_string(),
            "qa.remove.description".to_string(),
            "adaptive_card.default.title".to_string(),
        ]
    }
}

#[cfg(target_arch = "wasm32")]
bindings::export!(Component with_types_in bindings);
#[cfg(target_arch = "wasm32")]
greentic_interfaces_guest::export_component_v060!(NodeCompat);

pub fn describe_payload() -> String {
    serde_json::json!({
        "component": {
            "name": COMPONENT_NAME,
            "org": COMPONENT_ORG,
            "version": COMPONENT_VERSION,
            "world": "greentic:component/component@0.6.0",
            "schemas": {
                "component": COMPONENT_SCHEMA_JSON.clone(),
                "input": INPUT_SCHEMA_JSON.clone(),
                "output": OUTPUT_SCHEMA_JSON.clone()
            }
        }
    })
    .to_string()
}

fn encode_cbor<T: serde::Serialize>(value: &T) -> Vec<u8> {
    canonical::to_canonical_cbor_allow_floats(value).expect("encode cbor")
}

fn decode_cbor<T: for<'de> serde::Deserialize<'de>>(bytes: &[u8]) -> Result<T, ComponentError> {
    canonical::from_cbor(bytes)
        .map_err(|err| ComponentError::InvalidInput(format!("failed to decode cbor: {err}")))
}

fn input_schema_ir() -> SchemaIr {
    SchemaIr::Object {
        properties: BTreeMap::from([(
            "input".to_string(),
            SchemaIr::String {
                min_len: Some(0),
                max_len: Some(8192),
                regex: None,
                format: None,
            },
        )]),
        required: vec!["input".to_string()],
        additional: AdditionalProperties::Forbid,
    }
}

fn output_schema_ir() -> SchemaIr {
    SchemaIr::Object {
        properties: BTreeMap::from([(
            "message".to_string(),
            SchemaIr::String {
                min_len: Some(0),
                max_len: Some(8192),
                regex: None,
                format: None,
            },
        )]),
        required: vec!["message".to_string()],
        additional: AdditionalProperties::Forbid,
    }
}

fn input_schema_cbor() -> Vec<u8> {
    encode_cbor(&input_schema_ir())
}

fn output_schema_cbor() -> Vec<u8> {
    encode_cbor(&output_schema_ir())
}

#[cfg(target_arch = "wasm32")]
fn config_schema_ir() -> SchemaIr {
    SchemaIr::Object {
        properties: BTreeMap::from([
            (
                "asset_base_path".to_string(),
                SchemaIr::String {
                    min_len: Some(0),
                    max_len: Some(4096),
                    regex: None,
                    format: None,
                },
            ),
            (
                "catalog_registry_file".to_string(),
                SchemaIr::String {
                    min_len: Some(0),
                    max_len: Some(4096),
                    regex: None,
                    format: None,
                },
            ),
        ]),
        required: Vec::new(),
        additional: AdditionalProperties::Forbid,
    }
}

#[cfg(target_arch = "wasm32")]
fn config_schema_cbor() -> Vec<u8> {
    encode_cbor(&config_schema_ir())
}

#[cfg(target_arch = "wasm32")]
fn component_info() -> ComponentInfo {
    ComponentInfo {
        id: format!("{COMPONENT_ORG}.{COMPONENT_NAME}"),
        version: COMPONENT_VERSION.to_string(),
        role: "tool".to_string(),
        display_name: Some(I18nText::new(
            "component.display_name",
            Some(COMPONENT_NAME.to_string()),
        )),
    }
}

#[cfg(target_arch = "wasm32")]
fn component_info_cbor() -> Vec<u8> {
    encode_cbor(&component_info())
}

#[cfg(target_arch = "wasm32")]
fn component_describe() -> ComponentDescribe {
    let input = input_schema_ir();
    let output = output_schema_ir();
    let config = config_schema_ir();
    let op_schema_hash = schema_hash(&input, &output, &config).unwrap_or_default();
    let required_capabilities = if cfg!(feature = "state-store") {
        vec!["host:state".to_string()]
    } else {
        Vec::new()
    };

    ComponentDescribe {
        info: component_info(),
        provided_capabilities: Vec::new(),
        required_capabilities,
        metadata: BTreeMap::new(),
        operations: vec![ComponentOperation {
            id: "card".to_string(),
            display_name: Some(I18nText::new("adaptive_card.operation.card", None)),
            input: ComponentRunInput { schema: input },
            output: ComponentRunOutput { schema: output },
            defaults: BTreeMap::new(),
            redactions: Vec::new(),
            constraints: BTreeMap::new(),
            schema_hash: op_schema_hash,
        }],
        config_schema: config,
    }
}

#[cfg(target_arch = "wasm32")]
fn component_describe_cbor() -> Vec<u8> {
    encode_cbor(&component_describe())
}

fn run_component_cbor(input: Vec<u8>, _state: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
    let input_json: Result<serde_json::Value, _> = decode_cbor(&input);
    let output_json = match input_json {
        Ok(value) => {
            let op = value
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("card");
            let raw = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
            handle_message(op, &raw)
        }
        Err(err) => error_payload(
            "en",
            "AC_SCHEMA_INVALID",
            "errors.invalid_cbor_invocation",
            Some(serde_json::Value::String(err.to_string())),
        ),
    };
    let parsed: serde_json::Value =
        serde_json::from_str(&output_json).unwrap_or_else(|_| serde_json::json!({}));
    (encode_cbor(&parsed), encode_cbor(&serde_json::json!({})))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum QaQuestionKindDef {
    Text,
    Choice(Vec<(&'static str, &'static str)>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QaQuestionDef {
    id: &'static str,
    label_key: &'static str,
    help_key: &'static str,
    kind: QaQuestionKindDef,
    required: bool,
}

fn qa_question_defs() -> Vec<QaQuestionDef> {
    vec![
        QaQuestionDef {
            id: "card_source",
            label_key: "qa.question.card_source.label",
            help_key: "qa.question.card_source.help",
            kind: QaQuestionKindDef::Choice(vec![
                ("asset", "qa.question.card_source.option.asset"),
                ("inline", "qa.question.card_source.option.inline"),
                ("catalog", "qa.question.card_source.option.catalog"),
            ]),
            required: true,
        },
        QaQuestionDef {
            id: "card_input",
            label_key: "qa.question.card_input.label",
            help_key: "qa.question.card_input.help",
            kind: QaQuestionKindDef::Text,
            required: true,
        },
    ]
}

fn qa_question_defs_for_mode(mode_key: &str) -> Vec<QaQuestionDef> {
    match mode_key {
        "default" | "setup" | "update" => qa_question_defs(),
        _ => Vec::new(),
    }
}

#[cfg(target_arch = "wasm32")]
fn qa_card_questions_for_mode(mode_key: &str) -> Vec<Question> {
    qa_question_defs_for_mode(mode_key)
        .into_iter()
        .map(|question| Question {
            id: question.id.to_string(),
            label: I18nText::new(question.label_key, None),
            help: Some(I18nText::new(question.help_key, None)),
            error: None,
            kind: match question.kind {
                QaQuestionKindDef::Text => QuestionKind::Text,
                QaQuestionKindDef::Choice(options) => QuestionKind::Choice {
                    options: options
                        .into_iter()
                        .map(|(value, label_key)| ChoiceOption {
                            value: value.to_string(),
                            label: I18nText::new(label_key, None),
                        })
                        .collect(),
                },
            },
            required: question.required,
            default: None,
        })
        .collect()
}

fn qa_apply_answers_json(current_config: &[u8], answers: &[u8]) -> serde_json::Value {
    let mut merged = match decode_cbor::<serde_json::Value>(current_config)
        .unwrap_or_else(|_| serde_json::json!({}))
    {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };

    let answer_map =
        match decode_cbor::<serde_json::Value>(answers).unwrap_or_else(|_| serde_json::json!({})) {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };

    let selected_source = answer_map
        .get("card_source")
        .and_then(|v| v.as_str())
        .or_else(|| merged.get("card_source").and_then(|v| v.as_str()))
        .unwrap_or("asset");

    let mut card_spec = merged
        .get("card_spec")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    if selected_source.eq_ignore_ascii_case("inline") {
        let inline_json = resolve_inline_card_json(
            answer_map
                .get("card_input")
                .or_else(|| answer_map.get("inline_card_json")),
            &card_spec,
        );
        card_spec.insert("inline_json".to_string(), inline_json);
        card_spec.remove("asset_path");
        card_spec.remove("catalog_name");
    } else if selected_source.eq_ignore_ascii_case("catalog") {
        let catalog_name = answer_map
            .get("card_input")
            .or_else(|| answer_map.get("catalog_name"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                card_spec
                    .get("catalog_name")
                    .and_then(|v| v.as_str())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| "default-card".to_string());
        card_spec.insert(
            "catalog_name".to_string(),
            serde_json::Value::String(catalog_name),
        );
        card_spec.remove("inline_json");
        card_spec.remove("asset_path");
    } else {
        let asset_path = answer_map
            .get("card_input")
            .or_else(|| answer_map.get("asset_path"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                card_spec
                    .get("asset_path")
                    .and_then(|v| v.as_str())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| "card.json".to_string());
        card_spec.insert(
            "asset_path".to_string(),
            serde_json::Value::String(asset_path),
        );
        card_spec.remove("inline_json");
        card_spec.remove("catalog_name");
    }

    merged.insert(
        "card_source".to_string(),
        serde_json::Value::String(if selected_source.eq_ignore_ascii_case("inline") {
            "inline".to_string()
        } else if selected_source.eq_ignore_ascii_case("catalog") {
            "catalog".to_string()
        } else {
            "asset".to_string()
        }),
    );
    merged.insert(
        "card_spec".to_string(),
        serde_json::Value::Object(card_spec),
    );
    merged
        .entry("mode".to_string())
        .or_insert_with(|| serde_json::Value::String("renderAndValidate".to_string()));

    serde_json::Value::Object(merged)
}

fn resolve_inline_card_json(
    answer: Option<&serde_json::Value>,
    existing_card_spec: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    if let Some(value) = answer {
        if let Some(raw) = value.as_str() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw)
                && (parsed.is_object() || parsed.is_array())
            {
                return parsed;
            }
        } else if value.is_object() || value.is_array() {
            return value.clone();
        }
    }

    if let Some(existing) = existing_card_spec.get("inline_json")
        && (existing.is_object() || existing.is_array())
    {
        return existing.clone();
    }

    serde_json::json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            {
                "type": "TextBlock",
                "text": "{{i18n:adaptive_card.default.title}}"
            }
        ]
    })
}

pub fn handle_message(operation: &str, input: &str) -> String {
    let value: serde_json::Value = match serde_json::from_str(input) {
        Ok(value) => value,
        Err(err) => {
            return error_payload(
                "en",
                "AC_SCHEMA_INVALID",
                "errors.invalid_json",
                Some(serde_json::Value::String(err.to_string())),
            );
        }
    };
    let request_locale = i18n::resolve_locale_from_raw(&value);
    let invocation_value =
        validation::locate_invocation_candidate(&value).unwrap_or_else(|| value.clone());
    let validation_mode = read_validation_mode(&value, &invocation_value);
    let mut validation_issues = if validation_mode == ValidationMode::Off {
        Vec::new()
    } else {
        validation::validate_invocation_schema(&invocation_value)
    };
    if validation_mode == ValidationMode::Error && !validation_issues.is_empty() {
        return validation_error_payload(&request_locale, &validation_issues, None);
    }

    let mut invocation = match parse_invocation_value(&value) {
        Ok(invocation) => invocation,
        Err(err) => {
            if !validation_issues.is_empty() {
                return validation_error_payload(
                    &request_locale,
                    &validation_issues,
                    Some(&err.to_string()),
                );
            }
            return error_payload(
                &request_locale,
                "AC_SCHEMA_INVALID",
                "errors.invalid_invocation",
                Some(serde_json::Value::String(err.to_string())),
            );
        }
    };
    if invocation.locale.is_none() {
        invocation.locale = Some(request_locale.clone());
    }
    let locale = i18n::resolve_locale(&invocation);
    // Allow the operation name to steer mode selection if the host provides it.
    if operation.eq_ignore_ascii_case("validate") {
        invocation.mode = InvocationMode::Validate;
    }
    match handle_invocation(invocation) {
        Ok(mut result) => {
            if validation_mode != ValidationMode::Off {
                result.validation_issues.append(&mut validation_issues);
            }
            serde_json::to_string(&result).unwrap_or_else(|err| {
                error_payload(
                    &locale,
                    "AC_INTERNAL_ERROR",
                    "errors.serialization_error",
                    Some(serde_json::Value::String(err.to_string())),
                )
            })
        }
        Err(err) => {
            if !validation_issues.is_empty() {
                return validation_error_payload(
                    &locale,
                    &validation_issues,
                    Some(&err.to_string()),
                );
            }
            error_payload_from_error(&locale, &err)
        }
    }
}

pub fn handle_invocation(
    mut invocation: AdaptiveCardInvocation,
) -> Result<AdaptiveCardResult, ComponentError> {
    let state_loaded = state_store::load_state_if_missing(&mut invocation, None)?;
    let state_read_hash = state_loaded.as_ref().and_then(trace::hash_value);
    if let Some(interaction) = invocation.interaction.as_ref()
        && interaction.enabled == Some(false)
    {
        invocation.interaction = None;
    }
    if invocation.interaction.is_some() {
        return handle_interaction(&invocation);
    }

    let rendered = render_card(&invocation)?;
    if invocation.validation_mode == ValidationMode::Error && !rendered.validation_issues.is_empty()
    {
        return Err(ComponentError::CardValidation(rendered.validation_issues));
    }
    let rendered_card = match invocation.mode {
        InvocationMode::Validate => None,
        InvocationMode::Render | InvocationMode::RenderAndValidate => Some(rendered.card),
    };

    let mut telemetry_events = Vec::new();
    if trace::trace_enabled() {
        let state_key = Some(state_store::state_key_for(&invocation, None));
        telemetry_events.push(trace::build_trace_event(
            &invocation,
            &rendered.asset_resolution,
            &rendered.binding_summary,
            None,
            state_key,
            state_read_hash,
            None,
        ));
    }

    Ok(AdaptiveCardResult {
        rendered_card,
        event: None,
        state_updates: Vec::new(),
        session_updates: Vec::new(),
        card_features: rendered.features,
        validation_issues: rendered.validation_issues,
        telemetry_events,
    })
}

#[derive(serde::Deserialize, Default)]
struct InvocationEnvelope {
    #[serde(default)]
    config: Option<AdaptiveCardInvocation>,
    #[serde(default)]
    payload: serde_json::Value,
    #[serde(default)]
    session: serde_json::Value,
    #[serde(default)]
    state: serde_json::Value,
    #[serde(default)]
    interaction: Option<CardInteraction>,
    #[serde(default)]
    mode: Option<InvocationMode>,
    #[serde(default)]
    #[serde(alias = "validationMode")]
    validation_mode: Option<ValidationMode>,
    #[serde(default)]
    node_id: Option<String>,
    #[serde(default)]
    locale: Option<String>,
    #[serde(default)]
    #[serde(alias = "i18n_locale")]
    i18n_locale: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "model::deserialize_canonical_invocation_envelope_opt")]
    envelope: Option<CanonicalInvocationEnvelope>,
}

fn parse_invocation_value(
    value: &serde_json::Value,
) -> Result<AdaptiveCardInvocation, ComponentError> {
    if let Some(invocation_value) = validation::locate_invocation_candidate(value) {
        return serde_json::from_value::<AdaptiveCardInvocation>(invocation_value)
            .map_err(ComponentError::Serde);
    }

    if let Some(inner) = value.get("config") {
        if let Ok(invocation) = serde_json::from_value::<AdaptiveCardInvocation>(inner.clone()) {
            return merge_envelope(invocation, value);
        }
        if let Some(card) = inner.get("card")
            && let Ok(invocation) = serde_json::from_value::<AdaptiveCardInvocation>(card.clone())
        {
            return merge_envelope(invocation, value);
        }
    }

    let mut env: InvocationEnvelope = serde_json::from_value(value.clone())?;
    if env.config.is_none()
        && let Ok(invocation) =
            serde_json::from_value::<AdaptiveCardInvocation>(env.payload.clone())
    {
        return Ok(invocation);
    }
    let config = env.config.take().unwrap_or_default();
    Ok(merge_envelope_struct(config, env))
}

fn merge_envelope(
    mut inv: AdaptiveCardInvocation,
    value: &serde_json::Value,
) -> Result<AdaptiveCardInvocation, ComponentError> {
    let env: serde_json::Value = value.clone();
    let payload = env
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let session = env
        .get("session")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let state = env.get("state").cloned().unwrap_or(serde_json::Value::Null);
    if let Some(node_id) = env.get("node_id").and_then(|v| v.as_str()) {
        inv.node_id = Some(node_id.to_string());
    }
    if let Some(locale) = env
        .get("locale")
        .or_else(|| env.get("i18n_locale"))
        .and_then(|v| v.as_str())
    {
        inv.locale = Some(locale.to_string());
    }
    if !payload.is_null() {
        inv.payload = payload;
    }
    if !session.is_null() {
        inv.session = session;
    }
    if !state.is_null() {
        inv.state = state;
    }
    if inv.interaction.is_none()
        && let Some(interaction) = env.get("interaction")
    {
        inv.interaction = serde_json::from_value(interaction.clone()).ok();
    }
    if let Some(mode) = env.get("mode")
        && let Ok(parsed) = serde_json::from_value::<InvocationMode>(mode.clone())
    {
        inv.mode = parsed;
    }
    if let Some(mode_value) = env
        .get("validation_mode")
        .or_else(|| env.get("validationMode"))
        && let Some(parsed) = parse_validation_mode(mode_value)
    {
        inv.validation_mode = parsed;
    }
    if let Some(envelope) = env.get("envelope") {
        inv.envelope = model::parse_canonical_invocation_envelope(envelope.clone());
    }
    Ok(inv)
}

fn merge_envelope_struct(
    mut inv: AdaptiveCardInvocation,
    env: InvocationEnvelope,
) -> AdaptiveCardInvocation {
    if inv.card_spec.inline_json.is_none()
        && let Ok(candidate) = serde_json::from_value::<AdaptiveCardInvocation>(env.payload.clone())
    {
        return candidate;
    }
    if env.node_id.is_some() {
        inv.node_id = env.node_id;
    }
    if env.locale.is_some() {
        inv.locale = env.locale;
    } else if env.i18n_locale.is_some() {
        inv.locale = env.i18n_locale;
    }
    if !env.payload.is_null() {
        inv.payload = env.payload;
    }
    if !env.session.is_null() {
        inv.session = env.session;
    }
    if !env.state.is_null() {
        inv.state = env.state;
    }
    if inv.interaction.is_none() {
        inv.interaction = env.interaction;
    }
    if let Some(mode) = env.mode {
        inv.mode = mode;
    }
    if let Some(mode) = env.validation_mode {
        inv.validation_mode = mode;
    }
    if env.envelope.is_some() {
        inv.envelope = env.envelope;
    }
    inv
}

fn error_payload(
    locale: &str,
    code: &str,
    msg_key: &str,
    details: Option<serde_json::Value>,
) -> String {
    error_payload_with_args(locale, code, msg_key, &[], details)
}

fn error_payload_with_args(
    locale: &str,
    code: &str,
    msg_key: &str,
    args: &[(&str, &str)],
    details: Option<serde_json::Value>,
) -> String {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "code".to_string(),
        serde_json::Value::String(code.to_string()),
    );
    payload.insert(
        "msg_key".to_string(),
        serde_json::Value::String(msg_key.to_string()),
    );
    payload.insert(
        "message".to_string(),
        serde_json::Value::String(i18n::tf(locale, msg_key, args)),
    );
    if let Some(details) = details {
        payload.insert("details".to_string(), details);
    }
    serde_json::json!({ "error": payload }).to_string()
}

fn validation_error_payload(
    locale: &str,
    issues: &[ValidationIssue],
    detail: Option<&str>,
) -> String {
    let details = serde_json::json!({ "validation_issues": issues });
    if let Some(detail) = detail {
        return error_payload_with_args(
            locale,
            "AC_SCHEMA_INVALID",
            "errors.invocation_schema_validation_failed_detail",
            &[("detail", detail)],
            Some(details),
        );
    }
    error_payload(
        locale,
        "AC_SCHEMA_INVALID",
        "errors.invocation_schema_validation_failed",
        Some(details),
    )
}

fn error_payload_from_error(locale: &str, err: &ComponentError) -> String {
    let issue_details = |code: &str, message: String, path: &str| {
        serde_json::json!({
            "validation_issues": [{
                "code": code,
                "message": message,
                "path": path
            }]
        })
    };
    match err {
        ComponentError::InvalidInput(message) => error_payload(
            locale,
            "AC_SCHEMA_INVALID",
            "errors.invalid_input",
            Some(issue_details("AC_SCHEMA_INVALID", message.clone(), "/")),
        ),
        ComponentError::Serde(inner) => error_payload(
            locale,
            "AC_SCHEMA_INVALID",
            "errors.invalid_input",
            Some(issue_details("AC_SCHEMA_INVALID", inner.to_string(), "/")),
        ),
        ComponentError::Io(inner) => error_payload(
            locale,
            "AC_SCHEMA_INVALID",
            "errors.io_error",
            Some(issue_details("AC_SCHEMA_INVALID", inner.to_string(), "/")),
        ),
        ComponentError::AssetNotFound(path) => error_payload(
            locale,
            "AC_ASSET_NOT_FOUND",
            "errors.asset_not_found",
            Some(issue_details(
                "AC_ASSET_NOT_FOUND",
                path.clone(),
                "/card_spec",
            )),
        ),
        ComponentError::AssetParse(message) => error_payload(
            locale,
            "AC_ASSET_PARSE_ERROR",
            "errors.asset_parse_error",
            Some(issue_details(
                "AC_ASSET_PARSE_ERROR",
                message.clone(),
                "/card_spec",
            )),
        ),
        ComponentError::Asset(message) => error_payload(
            locale,
            "AC_ASSET_NOT_FOUND",
            "errors.asset_error",
            Some(issue_details(
                "AC_ASSET_NOT_FOUND",
                message.clone(),
                "/card_spec",
            )),
        ),
        ComponentError::Binding(message) => error_payload(
            locale,
            "AC_BINDING_EVAL_ERROR",
            "errors.binding_eval_error",
            Some(issue_details(
                "AC_BINDING_EVAL_ERROR",
                message.clone(),
                "/card_spec/inline_json",
            )),
        ),
        ComponentError::CardValidation(issues) => {
            let details = serde_json::json!({ "validation_issues": issues });
            error_payload(
                locale,
                "AC_CARD_VALIDATION_FAILED",
                "errors.card_validation_failed",
                Some(details),
            )
        }
        ComponentError::InteractionInvalid(message) => error_payload(
            locale,
            "AC_INTERACTION_INVALID",
            "errors.interaction_invalid",
            Some(issue_details(
                "AC_INTERACTION_INVALID",
                message.clone(),
                "/interaction",
            )),
        ),
        ComponentError::StateStore(message) => error_payload(
            locale,
            "AC_SCHEMA_INVALID",
            "errors.state_store_error",
            Some(issue_details(
                "AC_SCHEMA_INVALID",
                message.clone(),
                "/state",
            )),
        ),
    }
}

fn read_validation_mode(
    value: &serde_json::Value,
    invocation_value: &serde_json::Value,
) -> ValidationMode {
    invocation_value
        .get("validation_mode")
        .or_else(|| invocation_value.get("validationMode"))
        .or_else(|| value.get("validation_mode"))
        .or_else(|| value.get("validationMode"))
        .and_then(parse_validation_mode)
        .unwrap_or_default()
}

fn parse_validation_mode(value: &serde_json::Value) -> Option<ValidationMode> {
    let raw = value.as_str()?.to_ascii_lowercase();
    match raw.as_str() {
        "off" => Some(ValidationMode::Off),
        "warn" => Some(ValidationMode::Warn),
        "error" => Some(ValidationMode::Error),
        _ => None,
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_payload_value() {
        let input = json!({
            "card_spec": {
                "inline_json": {
                    "type": "AdaptiveCard",
                    "version": "1.3",
                    "body": [
                        { "type": "TextBlock", "text": "@{payload.title}" }
                    ]
                }
            },
            "payload": {
                "title": "Hello"
            }
        });
        let invocation = parse_invocation_value(&input).expect("should parse");
        println!("payload: {}", invocation.payload);
    }

    #[test]
    fn qa_apply_answers_builds_asset_invocation() {
        let answers = encode_cbor(&json!({
            "card_source": "asset",
            "card_input": "assets/cards/welcome.json"
        }));

        let merged = qa_apply_answers_json(&encode_cbor(&json!({})), &answers);
        assert_eq!(merged["card_source"], "asset");
        assert_eq!(
            merged["card_spec"]["asset_path"],
            "assets/cards/welcome.json"
        );
        assert_eq!(merged["mode"], "renderAndValidate");
    }

    #[test]
    fn qa_apply_answers_falls_back_to_i18n_inline_template() {
        let answers = encode_cbor(&json!({
            "card_source": "inline",
            "card_input": "not-json"
        }));

        let merged = qa_apply_answers_json(&encode_cbor(&json!({})), &answers);
        assert_eq!(merged["card_source"], "inline");
        assert_eq!(
            merged["card_spec"]["inline_json"]["body"][0]["text"],
            "{{i18n:adaptive_card.default.title}}"
        );
    }

    #[test]
    fn component_version_reflects_cargo_version() {
        assert_eq!(COMPONENT_VERSION, env!("CARGO_PKG_VERSION"));
        let payload: serde_json::Value =
            serde_json::from_str(&describe_payload()).expect("describe payload json");
        assert_eq!(
            payload["component"]["version"],
            serde_json::Value::String(env!("CARGO_PKG_VERSION").to_string())
        );
    }

    #[test]
    fn qa_questions_exist_for_default_setup_update_modes() {
        for mode in ["default", "setup", "update"] {
            let defs = qa_question_defs_for_mode(mode);
            assert_eq!(defs.len(), 2, "expected two questions for mode {mode}");
            assert_eq!(defs[0].id, "card_source");
            assert_eq!(defs[1].id, "card_input");
        }
    }

    #[test]
    fn qa_questions_for_remove_mode_are_empty() {
        assert!(qa_question_defs_for_mode("remove").is_empty());
    }

    #[test]
    fn qa_card_source_choices_cover_inline_asset_catalog() {
        let defs = qa_question_defs_for_mode("default");
        let source = defs
            .iter()
            .find(|q| q.id == "card_source")
            .expect("card_source question");
        let QaQuestionKindDef::Choice(choices) = &source.kind else {
            panic!("card_source must be a choice question");
        };
        let values: Vec<&str> = choices.iter().map(|(value, _)| *value).collect();
        assert_eq!(values, vec!["asset", "inline", "catalog"]);
    }

    #[test]
    fn qa_apply_answers_builds_inline_invocation_from_card_input() {
        let answers = encode_cbor(&json!({
            "card_source": "inline",
            "card_input": "{\"type\":\"AdaptiveCard\",\"version\":\"1.6\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"Hello\"}]}"
        }));

        let merged = qa_apply_answers_json(&encode_cbor(&json!({})), &answers);
        assert_eq!(merged["card_source"], "inline");
        assert_eq!(merged["card_spec"]["inline_json"]["type"], "AdaptiveCard");
        assert_eq!(
            merged["card_spec"]["inline_json"]["body"][0]["text"],
            "Hello"
        );
        assert_eq!(merged["mode"], "renderAndValidate");
    }

    #[test]
    fn qa_apply_answers_builds_catalog_invocation_from_card_input() {
        let answers = encode_cbor(&json!({
            "card_source": "catalog",
            "card_input": "welcome-card"
        }));

        let merged = qa_apply_answers_json(&encode_cbor(&json!({})), &answers);
        assert_eq!(merged["card_source"], "catalog");
        assert_eq!(merged["card_spec"]["catalog_name"], "welcome-card");
        assert!(merged["card_spec"].get("asset_path").is_none());
        assert!(merged["card_spec"].get("inline_json").is_none());
        assert_eq!(merged["mode"], "renderAndValidate");
    }

    #[test]
    fn qa_required_answers_are_defined_for_default_setup_update() {
        for mode in ["default", "setup", "update"] {
            let defs = qa_question_defs_for_mode(mode);
            assert!(
                defs.iter().any(|q| q.id == "card_source" && q.required),
                "card_source should be required for {mode}"
            );
            assert!(
                defs.iter().any(|q| q.id == "card_input" && q.required),
                "card_input should be required for {mode}"
            );
        }
    }
}
