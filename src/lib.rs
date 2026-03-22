#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod asset_resolver;
mod config;
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
use greentic_types::schemas::component::v0_6_0::{
    ChoiceOption, I18nText, Question, QuestionKind, SkipCondition, SkipExpression,
};
#[cfg(target_arch = "wasm32")]
use greentic_types::schemas::component::v0_6_0::{
    ComponentDescribe, ComponentInfo, ComponentOperation, ComponentQaSpec, ComponentRunInput,
    ComponentRunOutput, QaMode as QaModeModel, schema_hash,
};
use once_cell::sync::Lazy;

pub use asset_resolver::{
    register_host_asset_callback, register_host_asset_map, register_host_asset_resolver,
};
use config::{
    RuntimeConfig, parse_component_config_from_value, resolve_runtime_config,
    supported_locale_codes,
};
pub use error::ComponentError;
pub use model::*;

const COMPONENT_NAME: &str = "component-adaptive-card";
const COMPONENT_ORG: &str = "ai.greentic";
const COMPONENT_VERSION: &str = env!("CARGO_PKG_VERSION");

static COMPONENT_SCHEMA_JSON: Lazy<serde_json::Value> = Lazy::new(|| {
    serde_json::from_str(include_str!("../schemas/component.schema.json"))
        .expect("failed to parse component schema")
});
static COMPONENT_MANIFEST_JSON: Lazy<serde_json::Value> = Lazy::new(|| {
    serde_json::from_str(include_str!("../component.manifest.json"))
        .expect("failed to parse component manifest")
});
static INPUT_SCHEMA_JSON: Lazy<serde_json::Value> =
    Lazy::new(|| operation_schema_from_manifest("card", "input_schema"));
static OUTPUT_SCHEMA_JSON: Lazy<serde_json::Value> =
    Lazy::new(|| operation_schema_from_manifest("card", "output_schema"));

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
        let mode_key = match mode {
            QaMode::Default => "default",
            QaMode::Setup => "setup",
            QaMode::Update => "update",
            QaMode::Remove => "remove",
        };
        let merged = qa_apply_answers_json(mode_key, &current_config, &answers);
        encode_cbor(&merged)
    }
}

#[cfg(target_arch = "wasm32")]
impl component_i18n::Guest for Component {
    fn i18n_keys() -> Vec<String> {
        vec![
            "qa.default.title".to_string(),
            "qa.default.description".to_string(),
            "qa.question.catalog_registry_ref.help".to_string(),
            "qa.question.catalog_registry_ref.label".to_string(),
            "qa.question.confirm_remove.help".to_string(),
            "qa.question.confirm_remove.label".to_string(),
            "qa.question.card_source.label".to_string(),
            "qa.question.card_source.help".to_string(),
            "qa.question.card_source.option.asset".to_string(),
            "qa.question.card_source.option.inline".to_string(),
            "qa.question.card_source.option.catalog".to_string(),
            "qa.question.default_card_asset.help".to_string(),
            "qa.question.default_card_asset.label".to_string(),
            "qa.question.default_card_inline.help".to_string(),
            "qa.question.default_card_inline.label".to_string(),
            "qa.question.direction_mode.help".to_string(),
            "qa.question.direction_mode.label".to_string(),
            "qa.question.direction_mode.option.auto".to_string(),
            "qa.question.direction_mode.option.ltr".to_string(),
            "qa.question.direction_mode.option.rtl".to_string(),
            "qa.question.language_mode.help".to_string(),
            "qa.question.language_mode.label".to_string(),
            "qa.question.language_mode.option.all".to_string(),
            "qa.question.language_mode.option.custom".to_string(),
            "qa.question.multilingual.help".to_string(),
            "qa.question.multilingual.label".to_string(),
            "qa.question.supported_locales.help".to_string(),
            "qa.question.supported_locales.label".to_string(),
            "qa.question.trace_capture_inputs.help".to_string(),
            "qa.question.trace_capture_inputs.label".to_string(),
            "qa.question.trace_enabled.help".to_string(),
            "qa.question.trace_enabled.label".to_string(),
            "qa.question.update_area.help".to_string(),
            "qa.question.update_area.label".to_string(),
            "qa.question.update_area.option.card_source".to_string(),
            "qa.question.update_area.option.direction".to_string(),
            "qa.question.update_area.option.languages".to_string(),
            "qa.question.update_area.option.tracing".to_string(),
            "qa.question.update_area.option.validation".to_string(),
            "qa.question.validation_mode.help".to_string(),
            "qa.question.validation_mode.label".to_string(),
            "qa.question.validation_mode.option.error".to_string(),
            "qa.question.validation_mode.option.off".to_string(),
            "qa.question.validation_mode.option.warn".to_string(),
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

fn operation_schema_from_manifest(operation_name: &str, schema_key: &str) -> serde_json::Value {
    COMPONENT_MANIFEST_JSON
        .get("operations")
        .and_then(serde_json::Value::as_array)
        .and_then(|operations| {
            operations.iter().find(|operation| {
                operation.get("name").and_then(serde_json::Value::as_str) == Some(operation_name)
            })
        })
        .and_then(|operation| operation.get(schema_key))
        .cloned()
        .unwrap_or_else(|| panic!("missing {schema_key} for operation '{operation_name}'"))
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
                "default_source".to_string(),
                SchemaIr::Enum {
                    values: vec![
                        ciborium::value::Value::Text("inline".to_string()),
                        ciborium::value::Value::Text("asset".to_string()),
                        ciborium::value::Value::Text("catalog".to_string()),
                    ],
                },
            ),
            (
                "default_card_inline".to_string(),
                SchemaIr::OneOf {
                    variants: vec![
                        SchemaIr::Object {
                            properties: BTreeMap::new(),
                            required: Vec::new(),
                            additional: AdditionalProperties::Allow,
                        },
                        SchemaIr::Array {
                            items: Box::new(SchemaIr::Ref {
                                id: "json-value".to_string(),
                            }),
                            min_items: None,
                            max_items: None,
                        },
                        SchemaIr::Null,
                    ],
                },
            ),
            (
                "default_card_asset".to_string(),
                SchemaIr::OneOf {
                    variants: vec![
                        SchemaIr::String {
                            min_len: Some(1),
                            max_len: Some(4096),
                            regex: None,
                            format: None,
                        },
                        SchemaIr::Null,
                    ],
                },
            ),
            (
                "catalog_registry_ref".to_string(),
                SchemaIr::OneOf {
                    variants: vec![
                        SchemaIr::String {
                            min_len: Some(1),
                            max_len: Some(4096),
                            regex: None,
                            format: None,
                        },
                        SchemaIr::Null,
                    ],
                },
            ),
            ("multilingual".to_string(), SchemaIr::Bool),
            (
                "language_mode".to_string(),
                SchemaIr::Enum {
                    values: vec![
                        ciborium::value::Value::Text("all".to_string()),
                        ciborium::value::Value::Text("custom".to_string()),
                    ],
                },
            ),
            (
                "supported_locales".to_string(),
                SchemaIr::OneOf {
                    variants: vec![
                        SchemaIr::Array {
                            items: Box::new(SchemaIr::String {
                                min_len: Some(2),
                                max_len: Some(32),
                                regex: None,
                                format: None,
                            }),
                            min_items: None,
                            max_items: None,
                        },
                        SchemaIr::Null,
                    ],
                },
            ),
            (
                "direction_mode".to_string(),
                SchemaIr::Enum {
                    values: vec![
                        ciborium::value::Value::Text("ltr".to_string()),
                        ciborium::value::Value::Text("rtl".to_string()),
                        ciborium::value::Value::Text("auto".to_string()),
                    ],
                },
            ),
            (
                "validation_mode".to_string(),
                SchemaIr::Enum {
                    values: vec![
                        ciborium::value::Value::Text("off".to_string()),
                        ciborium::value::Value::Text("warn".to_string()),
                        ciborium::value::Value::Text("error".to_string()),
                    ],
                },
            ),
            ("trace_enabled".to_string(), SchemaIr::Bool),
            ("trace_capture_inputs".to_string(), SchemaIr::Bool),
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

fn qa_card_questions_for_mode(mode_key: &str) -> Vec<Question> {
    match mode_key {
        "default" => qa_default_questions(),
        "setup" => qa_setup_questions(),
        "update" => qa_update_questions(),
        "remove" => qa_remove_questions(),
        _ => Vec::new(),
    }
}

fn qa_default_questions() -> Vec<Question> {
    let mut questions = source_questions();
    questions.push(multilingual_question());
    questions.push(language_mode_question(Some(skip_if_not_equals(
        "multilingual",
        true,
    ))));
    questions.push(custom_locales_question(Some(skip_if_not_equals(
        "language_mode",
        "custom",
    ))));
    questions
}

fn qa_setup_questions() -> Vec<Question> {
    let mut questions = qa_default_questions();
    questions.push(direction_question(None));
    questions.push(validation_question(None));
    questions.push(trace_enabled_question(None));
    questions.push(trace_capture_question(Some(skip_if_not_equals(
        "trace_enabled",
        true,
    ))));
    questions
}

fn qa_update_questions() -> Vec<Question> {
    let mut questions = vec![update_area_question()];
    let card_skip = skip_if_not_equals("update_area", "card_source");
    questions.push(source_choice_question(Some(card_skip.clone())));
    questions.push(inline_card_question(Some(skip_if_not_card_source(
        "inline",
        "card_source",
    ))));
    questions.push(asset_card_question(Some(skip_if_not_card_source(
        "asset",
        "card_source",
    ))));
    questions.push(catalog_ref_question(Some(skip_if_not_card_source(
        "catalog",
        "card_source",
    ))));

    let languages_skip = skip_if_not_equals("update_area", "languages");
    questions.push(multilingual_question_with_skip(Some(
        languages_skip.clone(),
    )));
    questions.push(language_mode_question(Some(SkipExpression::Or(vec![
        languages_skip.clone(),
        skip_if_not_equals("multilingual", true),
    ]))));
    questions.push(custom_locales_question(Some(SkipExpression::Or(vec![
        languages_skip,
        skip_if_not_equals("language_mode", "custom"),
    ]))));

    questions.push(direction_question(Some(skip_if_not_equals(
        "update_area",
        "direction",
    ))));
    questions.push(validation_question(Some(skip_if_not_equals(
        "update_area",
        "validation",
    ))));
    questions.push(trace_enabled_question(Some(skip_if_not_equals(
        "update_area",
        "tracing",
    ))));
    questions.push(trace_capture_question(Some(SkipExpression::Or(vec![
        skip_if_not_equals("update_area", "tracing"),
        skip_if_not_equals("trace_enabled", true),
    ]))));
    questions
}

fn qa_remove_questions() -> Vec<Question> {
    vec![Question {
        id: "confirm_remove".to_string(),
        label: i18n_text("qa.question.confirm_remove.label"),
        help: Some(i18n_text("qa.question.confirm_remove.help")),
        error: None,
        kind: QuestionKind::Bool,
        required: true,
        default: Some(ciborium::value::Value::Bool(false)),
        skip_if: None,
    }]
}

fn source_questions() -> Vec<Question> {
    vec![
        source_choice_question(None),
        inline_card_question(Some(skip_if_not_equals("card_source", "inline"))),
        asset_card_question(Some(skip_if_not_equals("card_source", "asset"))),
        catalog_ref_question(Some(skip_if_not_equals("card_source", "catalog"))),
    ]
}

fn source_choice_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "card_source".to_string(),
        label: i18n_text("qa.question.card_source.label"),
        help: Some(i18n_text("qa.question.card_source.help")),
        error: None,
        kind: QuestionKind::Choice {
            options: vec![
                choice("inline", "qa.question.card_source.option.inline"),
                choice("asset", "qa.question.card_source.option.asset"),
                choice("catalog", "qa.question.card_source.option.catalog"),
            ],
        },
        required: true,
        default: Some(ciborium::value::Value::Text("inline".to_string())),
        skip_if,
    }
}

fn inline_card_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "default_card_inline".to_string(),
        label: i18n_text("qa.question.default_card_inline.label"),
        help: Some(i18n_text("qa.question.default_card_inline.help")),
        error: None,
        kind: QuestionKind::InlineJson { schema: None },
        required: true,
        default: Some(json_to_cbor_value(&default_inline_card_json())),
        skip_if,
    }
}

fn asset_card_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "default_card_asset".to_string(),
        label: i18n_text("qa.question.default_card_asset.label"),
        help: Some(i18n_text("qa.question.default_card_asset.help")),
        error: None,
        kind: QuestionKind::AssetRef {
            file_types: vec!["json".to_string()],
            base_path: Some("assets".to_string()),
            check_exists: false,
        },
        required: true,
        default: None,
        skip_if,
    }
}

fn catalog_ref_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "catalog_registry_ref".to_string(),
        label: i18n_text("qa.question.catalog_registry_ref.label"),
        help: Some(i18n_text("qa.question.catalog_registry_ref.help")),
        error: None,
        kind: QuestionKind::Text,
        required: true,
        default: None,
        skip_if,
    }
}

fn multilingual_question() -> Question {
    multilingual_question_with_skip(None)
}

fn multilingual_question_with_skip(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "multilingual".to_string(),
        label: i18n_text("qa.question.multilingual.label"),
        help: Some(i18n_text("qa.question.multilingual.help")),
        error: None,
        kind: QuestionKind::Bool,
        required: true,
        default: Some(ciborium::value::Value::Bool(true)),
        skip_if,
    }
}

fn language_mode_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "language_mode".to_string(),
        label: i18n_text("qa.question.language_mode.label"),
        help: Some(i18n_text("qa.question.language_mode.help")),
        error: None,
        kind: QuestionKind::Choice {
            options: vec![
                choice("all", "qa.question.language_mode.option.all"),
                choice("custom", "qa.question.language_mode.option.custom"),
            ],
        },
        required: true,
        default: Some(ciborium::value::Value::Text("all".to_string())),
        skip_if,
    }
}

fn custom_locales_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "supported_locales".to_string(),
        label: i18n_text("qa.question.supported_locales.label"),
        help: Some(i18n_text("qa.question.supported_locales.help")),
        error: None,
        kind: QuestionKind::Text,
        required: true,
        default: Some(ciborium::value::Value::Text(
            "en,en-GB,fr,de,nl".to_string(),
        )),
        skip_if,
    }
}

fn direction_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "direction_mode".to_string(),
        label: i18n_text("qa.question.direction_mode.label"),
        help: Some(i18n_text("qa.question.direction_mode.help")),
        error: None,
        kind: QuestionKind::Choice {
            options: vec![
                choice("ltr", "qa.question.direction_mode.option.ltr"),
                choice("rtl", "qa.question.direction_mode.option.rtl"),
                choice("auto", "qa.question.direction_mode.option.auto"),
            ],
        },
        required: true,
        default: Some(ciborium::value::Value::Text("ltr".to_string())),
        skip_if,
    }
}

fn validation_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "validation_mode".to_string(),
        label: i18n_text("qa.question.validation_mode.label"),
        help: Some(i18n_text("qa.question.validation_mode.help")),
        error: None,
        kind: QuestionKind::Choice {
            options: vec![
                choice("off", "qa.question.validation_mode.option.off"),
                choice("warn", "qa.question.validation_mode.option.warn"),
                choice("error", "qa.question.validation_mode.option.error"),
            ],
        },
        required: true,
        default: Some(ciborium::value::Value::Text("warn".to_string())),
        skip_if,
    }
}

fn trace_enabled_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "trace_enabled".to_string(),
        label: i18n_text("qa.question.trace_enabled.label"),
        help: Some(i18n_text("qa.question.trace_enabled.help")),
        error: None,
        kind: QuestionKind::Bool,
        required: true,
        default: Some(ciborium::value::Value::Bool(false)),
        skip_if,
    }
}

fn trace_capture_question(skip_if: Option<SkipExpression>) -> Question {
    Question {
        id: "trace_capture_inputs".to_string(),
        label: i18n_text("qa.question.trace_capture_inputs.label"),
        help: Some(i18n_text("qa.question.trace_capture_inputs.help")),
        error: None,
        kind: QuestionKind::Bool,
        required: true,
        default: Some(ciborium::value::Value::Bool(false)),
        skip_if,
    }
}

fn update_area_question() -> Question {
    Question {
        id: "update_area".to_string(),
        label: i18n_text("qa.question.update_area.label"),
        help: Some(i18n_text("qa.question.update_area.help")),
        error: None,
        kind: QuestionKind::Choice {
            options: vec![
                choice("card_source", "qa.question.update_area.option.card_source"),
                choice("languages", "qa.question.update_area.option.languages"),
                choice("direction", "qa.question.update_area.option.direction"),
                choice("validation", "qa.question.update_area.option.validation"),
                choice("tracing", "qa.question.update_area.option.tracing"),
            ],
        },
        required: true,
        default: None,
        skip_if: None,
    }
}

fn qa_apply_answers_json(
    mode_key: &str,
    current_config: &[u8],
    answers: &[u8],
) -> serde_json::Value {
    let mut merged = decode_object_map(current_config);
    let answer_map = decode_object_map(answers);

    if mode_key == "remove" {
        return if answer_map
            .get("confirm_remove")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            serde_json::json!({})
        } else {
            serde_json::Value::Object(merged)
        };
    }

    let update_area = answer_map
        .get("update_area")
        .and_then(serde_json::Value::as_str);
    let should_update = |area: &str| mode_key != "update" || update_area == Some(area);

    if should_update("card_source") {
        apply_source_answers(&mut merged, &answer_map);
    }
    if should_update("languages") {
        apply_language_answers(&mut merged, &answer_map);
    }
    if mode_key == "setup" || should_update("direction") {
        apply_choice_answer(&mut merged, &answer_map, "direction_mode", "ltr");
    } else {
        merged
            .entry("direction_mode".to_string())
            .or_insert_with(|| serde_json::Value::String("ltr".to_string()));
    }
    if mode_key == "setup" || should_update("validation") {
        apply_choice_answer(&mut merged, &answer_map, "validation_mode", "warn");
    } else {
        merged
            .entry("validation_mode".to_string())
            .or_insert_with(|| serde_json::Value::String("warn".to_string()));
    }
    if mode_key == "setup" || should_update("tracing") {
        apply_bool_answer(&mut merged, &answer_map, "trace_enabled", false);
        apply_bool_answer(&mut merged, &answer_map, "trace_capture_inputs", false);
    } else {
        merged
            .entry("trace_enabled".to_string())
            .or_insert_with(|| serde_json::Value::Bool(false));
        merged
            .entry("trace_capture_inputs".to_string())
            .or_insert_with(|| serde_json::Value::Bool(false));
    }

    serde_json::Value::Object(merged)
}

fn apply_source_answers(
    merged: &mut serde_json::Map<String, serde_json::Value>,
    answers: &serde_json::Map<String, serde_json::Value>,
) {
    let selected_source = answers
        .get("card_source")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            merged
                .get("default_source")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("inline")
        .to_ascii_lowercase();

    merged.insert(
        "default_source".to_string(),
        serde_json::Value::String(selected_source.clone()),
    );

    match selected_source.as_str() {
        "inline" => {
            let inline_json = resolve_inline_card_json(
                answers.get("default_card_inline"),
                merged.get("default_card_inline"),
            );
            merged.insert("default_card_inline".to_string(), inline_json);
            merged.insert("default_card_asset".to_string(), serde_json::Value::Null);
            merged
                .entry("catalog_registry_ref".to_string())
                .or_insert(serde_json::Value::Null);
        }
        "asset" => {
            let asset = string_answer(answers, "default_card_asset")
                .or_else(|| string_value_json(merged.get("default_card_asset")));
            merged.insert(
                "default_card_asset".to_string(),
                asset
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            merged.insert("default_card_inline".to_string(), serde_json::Value::Null);
            merged
                .entry("catalog_registry_ref".to_string())
                .or_insert(serde_json::Value::Null);
        }
        "catalog" => {
            let reference = string_answer(answers, "catalog_registry_ref")
                .or_else(|| string_value_json(merged.get("catalog_registry_ref")));
            merged.insert(
                "catalog_registry_ref".to_string(),
                reference
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            merged.insert("default_card_inline".to_string(), serde_json::Value::Null);
            merged.insert("default_card_asset".to_string(), serde_json::Value::Null);
        }
        _ => {}
    }
}

fn apply_language_answers(
    merged: &mut serde_json::Map<String, serde_json::Value>,
    answers: &serde_json::Map<String, serde_json::Value>,
) {
    let multilingual = answers
        .get("multilingual")
        .and_then(serde_json::Value::as_bool)
        .or_else(|| {
            merged
                .get("multilingual")
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(true);
    merged.insert(
        "multilingual".to_string(),
        serde_json::Value::Bool(multilingual),
    );

    if !multilingual {
        merged.insert(
            "language_mode".to_string(),
            serde_json::Value::String("all".to_string()),
        );
        merged.insert("supported_locales".to_string(), serde_json::Value::Null);
        return;
    }

    let language_mode = answers
        .get("language_mode")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            merged
                .get("language_mode")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("all");
    let language_mode = language_mode.to_string();
    merged.insert(
        "language_mode".to_string(),
        serde_json::Value::String(language_mode.clone()),
    );

    if language_mode.eq_ignore_ascii_case("custom") {
        let locales = parse_locales_answer(answers.get("supported_locales"))
            .or_else(|| merged.get("supported_locales").cloned())
            .unwrap_or_else(|| {
                serde_json::Value::Array(vec![
                    serde_json::Value::String("en".to_string()),
                    serde_json::Value::String("en-GB".to_string()),
                    serde_json::Value::String("fr".to_string()),
                    serde_json::Value::String("de".to_string()),
                    serde_json::Value::String("nl".to_string()),
                ])
            });
        merged.insert("supported_locales".to_string(), locales);
    } else {
        merged.insert("supported_locales".to_string(), serde_json::Value::Null);
    }
}

fn resolve_inline_card_json(
    answer: Option<&serde_json::Value>,
    existing: Option<&serde_json::Value>,
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

    if let Some(existing) = existing
        && (existing.is_object() || existing.is_array())
    {
        return existing.clone();
    }

    default_inline_card_json()
}

fn default_inline_card_json() -> serde_json::Value {
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

fn decode_object_map(bytes: &[u8]) -> serde_json::Map<String, serde_json::Value> {
    match decode_cbor::<serde_json::Value>(bytes).unwrap_or_else(|_| serde_json::json!({})) {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}

fn apply_choice_answer(
    merged: &mut serde_json::Map<String, serde_json::Value>,
    answers: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default_value: &str,
) {
    let value = answers
        .get(key)
        .and_then(serde_json::Value::as_str)
        .or_else(|| merged.get(key).and_then(serde_json::Value::as_str))
        .unwrap_or(default_value);
    merged.insert(
        key.to_string(),
        serde_json::Value::String(value.to_string()),
    );
}

fn apply_bool_answer(
    merged: &mut serde_json::Map<String, serde_json::Value>,
    answers: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default_value: bool,
) {
    let value = answers
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .or_else(|| merged.get(key).and_then(serde_json::Value::as_bool))
        .unwrap_or(default_value);
    merged.insert(key.to_string(), serde_json::Value::Bool(value));
}

fn parse_locales_answer(value: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    let raw = value?;
    let tokens = if let Some(text) = raw.as_str() {
        text.split(',').collect::<Vec<_>>()
    } else if let Some(items) = raw.as_array() {
        return Some(serde_json::Value::Array(
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter_map(|locale| {
                    config::resolve_locale_against(locale, supported_locale_codes())
                })
                .map(serde_json::Value::String)
                .collect(),
        ));
    } else {
        return None;
    };

    let mut locales = Vec::new();
    for token in tokens {
        if let Some(locale) = config::resolve_locale_against(token, supported_locale_codes())
            && !locales
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&locale))
        {
            locales.push(locale);
        }
    }
    Some(serde_json::Value::Array(
        locales.into_iter().map(serde_json::Value::String).collect(),
    ))
}

fn string_answer(
    answers: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    answers
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn string_value_json(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn i18n_text(key: &str) -> I18nText {
    I18nText::new(key.to_string(), None)
}

fn choice(value: &str, label_key: &str) -> ChoiceOption {
    ChoiceOption {
        value: value.to_string(),
        label: i18n_text(label_key),
    }
}

fn json_to_cbor_value(value: &serde_json::Value) -> ciborium::value::Value {
    match value {
        serde_json::Value::Null => ciborium::value::Value::Null,
        serde_json::Value::Bool(flag) => ciborium::value::Value::Bool(*flag),
        serde_json::Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                ciborium::value::Value::Integer(value.into())
            } else if let Some(value) = number.as_u64() {
                ciborium::value::Value::Integer(value.into())
            } else {
                ciborium::value::Value::Float(number.as_f64().unwrap_or_default())
            }
        }
        serde_json::Value::String(text) => ciborium::value::Value::Text(text.clone()),
        serde_json::Value::Array(items) => {
            ciborium::value::Value::Array(items.iter().map(json_to_cbor_value).collect())
        }
        serde_json::Value::Object(map) => ciborium::value::Value::Map(
            map.iter()
                .map(|(key, value)| {
                    (
                        ciborium::value::Value::Text(key.clone()),
                        json_to_cbor_value(value),
                    )
                })
                .collect(),
        ),
    }
}

fn skip_if_not_equals(field: &str, value: impl Into<serde_json::Value>) -> SkipExpression {
    SkipExpression::Condition(SkipCondition {
        field: field.to_string(),
        equals: None,
        not_equals: Some(json_to_cbor_value(&value.into())),
        is_empty: false,
        is_not_empty: false,
    })
}

fn skip_if_not_card_source(source: &str, field: &str) -> SkipExpression {
    skip_if_not_equals(field, source)
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
    let component_config = parse_component_config_from_value(&value);
    let runtime_config = resolve_runtime_config(component_config.as_ref());
    let request_locale = i18n::resolve_locale_from_raw_with_config(&value, &runtime_config);
    let invocation_value =
        validation::locate_invocation_candidate(&value).unwrap_or_else(|| serde_json::json!({}));
    let validation_mode = read_validation_mode(&value, &invocation_value, &runtime_config);
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
    apply_runtime_defaults(&mut invocation, &invocation_value, &runtime_config);
    if invocation.locale.is_none() {
        invocation.locale = Some(request_locale.clone());
    }
    let locale = i18n::resolve_locale_with_config(&invocation, &runtime_config);
    // Allow the operation name to steer mode selection if the host provides it.
    if operation.eq_ignore_ascii_case("validate") {
        invocation.mode = InvocationMode::Validate;
    }
    match handle_invocation_with_config(invocation, &runtime_config) {
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
    invocation: AdaptiveCardInvocation,
) -> Result<AdaptiveCardResult, ComponentError> {
    handle_invocation_with_config(invocation, &RuntimeConfig::default())
}

fn handle_invocation_with_config(
    mut invocation: AdaptiveCardInvocation,
    runtime_config: &RuntimeConfig,
) -> Result<AdaptiveCardResult, ComponentError> {
    let state_loaded = state_store::load_state_if_missing(&mut invocation, None)?;
    let state_read_hash = state_loaded.as_ref().and_then(trace::hash_value);
    if let Some(interaction) = invocation.interaction.as_ref()
        && interaction.enabled == Some(false)
    {
        invocation.interaction = None;
    }
    if invocation.interaction.is_some() {
        return interaction::handle_interaction(&invocation, runtime_config);
    }

    let rendered = render::render_card(&invocation, runtime_config)?;
    if invocation.validation_mode == ValidationMode::Error && !rendered.validation_issues.is_empty()
    {
        return Err(ComponentError::CardValidation(rendered.validation_issues));
    }
    let rendered_card = match invocation.mode {
        InvocationMode::Validate => None,
        InvocationMode::Render | InvocationMode::RenderAndValidate => Some(rendered.card),
    };

    let mut telemetry_events = Vec::new();
    if runtime_config.trace_enabled {
        let state_key = Some(state_store::state_key_for(&invocation, None));
        telemetry_events.push(trace::build_trace_event(
            &invocation,
            &rendered.asset_resolution,
            &rendered.binding_summary,
            &trace::TraceContext {
                interaction: None,
                state_key,
                state_read_hash,
                state_write_hash: None,
                capture_inputs: runtime_config.trace_capture_inputs,
            },
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
    config: Option<serde_json::Value>,
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
        if validation::locate_invocation_candidate(inner).is_some()
            && let Ok(invocation) = serde_json::from_value::<AdaptiveCardInvocation>(inner.clone())
        {
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
    let config = env
        .config
        .take()
        .and_then(|inner| {
            validation::locate_invocation_candidate(&inner).and_then(|candidate| {
                serde_json::from_value::<AdaptiveCardInvocation>(candidate).ok()
            })
        })
        .unwrap_or_default();
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

fn apply_runtime_defaults(
    invocation: &mut AdaptiveCardInvocation,
    invocation_value: &serde_json::Value,
    runtime_config: &RuntimeConfig,
) {
    let explicit_card_source = invocation_value.get("card_source").is_some();
    let explicit_card_spec = invocation_value
        .get("card_spec")
        .and_then(serde_json::Value::as_object);

    if !explicit_card_source {
        invocation.card_source = runtime_config.default_source.clone();
    }

    match invocation.card_source {
        CardSource::Inline => {
            let explicit_inline = explicit_card_spec
                .and_then(|spec| spec.get("inline_json"))
                .filter(|value| value.is_object() || value.is_array())
                .cloned();
            if let Some(inline) = explicit_inline {
                invocation.card_spec.inline_json = Some(inline);
            } else if invocation.card_spec.inline_json.is_none() {
                invocation.card_spec.inline_json = runtime_config
                    .default_card_inline
                    .clone()
                    .or_else(|| Some(default_inline_card_json()));
            }
        }
        CardSource::Asset => {
            let explicit_asset = explicit_card_spec
                .and_then(|spec| spec.get("asset_path"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            if explicit_asset.is_none() && invocation.card_spec.asset_path.is_none() {
                invocation.card_spec.asset_path = runtime_config.default_card_asset.clone();
            }
        }
        CardSource::Catalog => {}
    }

    let explicit_validation_mode = invocation_value
        .get("validation_mode")
        .or_else(|| invocation_value.get("validationMode"))
        .is_some();
    if !explicit_validation_mode {
        invocation.validation_mode = runtime_config.validation_mode.clone();
    }
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
    runtime_config: &RuntimeConfig,
) -> ValidationMode {
    invocation_value
        .get("validation_mode")
        .or_else(|| invocation_value.get("validationMode"))
        .or_else(|| value.get("validation_mode"))
        .or_else(|| value.get("validationMode"))
        .and_then(parse_validation_mode)
        .unwrap_or_else(|| runtime_config.validation_mode.clone())
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
    fn qa_apply_answers_builds_asset_config() {
        let answers = encode_cbor(&json!({
            "card_source": "asset",
            "default_card_asset": "assets/cards/welcome.json",
            "multilingual": true,
            "language_mode": "all"
        }));

        let merged = qa_apply_answers_json("default", &encode_cbor(&json!({})), &answers);
        assert_eq!(merged["default_source"], "asset");
        assert_eq!(merged["default_card_asset"], "assets/cards/welcome.json");
        assert_eq!(merged["direction_mode"], "ltr");
    }

    #[test]
    fn qa_apply_answers_falls_back_to_i18n_inline_template() {
        let answers = encode_cbor(&json!({
            "card_source": "inline",
            "default_card_inline": "not-json",
            "multilingual": false
        }));

        let merged = qa_apply_answers_json("default", &encode_cbor(&json!({})), &answers);
        assert_eq!(merged["default_source"], "inline");
        assert_eq!(
            merged["default_card_inline"]["body"][0]["text"],
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
    fn qa_default_questions_cover_source_and_languages() {
        let questions = qa_card_questions_for_mode("default");
        let ids: Vec<&str> = questions
            .iter()
            .map(|question| question.id.as_str())
            .collect();
        assert!(ids.contains(&"card_source"));
        assert!(ids.contains(&"default_card_inline"));
        assert!(ids.contains(&"default_card_asset"));
        assert!(ids.contains(&"catalog_registry_ref"));
        assert!(ids.contains(&"multilingual"));
        assert!(ids.contains(&"language_mode"));
        assert!(ids.contains(&"supported_locales"));
    }

    #[test]
    fn qa_remove_questions_confirm_removal() {
        let questions = qa_card_questions_for_mode("remove");
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].id, "confirm_remove");
    }

    #[test]
    fn qa_card_source_choices_cover_inline_asset_catalog() {
        let questions = qa_card_questions_for_mode("default");
        let source = questions
            .iter()
            .find(|q| q.id == "card_source")
            .expect("card_source question");
        let QuestionKind::Choice { options } = &source.kind else {
            panic!("card_source must be a choice question");
        };
        let values: Vec<&str> = options.iter().map(|option| option.value.as_str()).collect();
        assert_eq!(values, vec!["inline", "asset", "catalog"]);
    }

    #[test]
    fn qa_apply_answers_builds_inline_config_from_card_input() {
        let answers = encode_cbor(&json!({
            "card_source": "inline",
            "default_card_inline": "{\"type\":\"AdaptiveCard\",\"version\":\"1.6\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"Hello\"}]}",
            "multilingual": true,
            "language_mode": "custom",
            "supported_locales": "en,en-GB,fr,de,nl"
        }));

        let merged = qa_apply_answers_json("default", &encode_cbor(&json!({})), &answers);
        assert_eq!(merged["default_source"], "inline");
        assert_eq!(merged["default_card_inline"]["type"], "AdaptiveCard");
        assert_eq!(merged["default_card_inline"]["body"][0]["text"], "Hello");
        assert_eq!(merged["supported_locales"][0], "en");
    }

    #[test]
    fn qa_apply_answers_builds_catalog_config_from_registry_ref() {
        let answers = encode_cbor(&json!({
            "card_source": "catalog",
            "catalog_registry_ref": "repo://my-repo/cards/catalog.json",
            "multilingual": true,
            "language_mode": "all"
        }));

        let merged = qa_apply_answers_json("default", &encode_cbor(&json!({})), &answers);
        assert_eq!(merged["default_source"], "catalog");
        assert_eq!(
            merged["catalog_registry_ref"],
            "repo://my-repo/cards/catalog.json"
        );
        assert_eq!(merged["default_card_inline"], serde_json::Value::Null);
        assert_eq!(merged["default_card_asset"], serde_json::Value::Null);
    }

    #[test]
    fn update_questions_start_with_update_area() {
        let questions = qa_card_questions_for_mode("update");
        assert_eq!(
            questions.first().map(|question| question.id.as_str()),
            Some("update_area")
        );
    }
}
