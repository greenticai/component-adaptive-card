use serde_json::{Map, Value};

use crate::model::{AdaptiveCardInvocation, CardInteraction, TelemetryEvent};
use crate::render::{AssetResolution, BindingSummary};

pub struct TraceContext {
    pub interaction: Option<CardInteraction>,
    pub state_key: Option<String>,
    pub state_read_hash: Option<String>,
    pub state_write_hash: Option<String>,
    pub capture_inputs: bool,
}

pub fn hash_value(value: &Value) -> Option<String> {
    let bytes = serde_json::to_vec(value).ok()?;
    Some(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}

pub fn build_trace_event(
    invocation: &AdaptiveCardInvocation,
    asset_resolution: &AssetResolution,
    binding_summary: &BindingSummary,
    trace_ctx: &TraceContext,
) -> TelemetryEvent {
    let mut properties = Map::new();
    properties.insert(
        "card_source".to_string(),
        serde_json::to_value(&invocation.card_source).unwrap_or(Value::Null),
    );
    properties.insert(
        "asset_resolution".to_string(),
        serde_json::json!({
            "mode": asset_resolution.mode,
            "resolved": asset_resolution.resolved,
            "asset_hash": asset_resolution.hash
        }),
    );
    properties.insert(
        "bindings_summary".to_string(),
        serde_json::json!({
            "handlebars_expansions": binding_summary.handlebars_expansions,
            "placeholder_replacements": binding_summary.placeholder_replacements,
            "expression_evaluations": binding_summary.expression_evaluations,
            "missing_paths": binding_summary.missing_paths
        }),
    );
    if let Some(interaction) = trace_ctx.interaction.as_ref() {
        properties.insert(
            "interaction_summary".to_string(),
            serde_json::json!({
                "type": interaction.interaction_type,
                "action_id": interaction.action_id,
                "card_instance_id": interaction.card_instance_id,
                "route": interaction.metadata.get("route").cloned()
            }),
        );
    }
    properties.insert(
        "state_summary".to_string(),
        serde_json::json!({
            "state_key": trace_ctx.state_key,
            "state_read_hash": trace_ctx.state_read_hash,
            "state_write_hash": trace_ctx.state_write_hash
        }),
    );

    if trace_ctx.capture_inputs {
        properties.insert(
            "inputs".to_string(),
            serde_json::json!({
                "payload": invocation.payload,
                "session": invocation.session,
                "state": invocation.state,
                "interaction_raw_inputs": trace_ctx
                    .interaction
                    .as_ref()
                    .map(|interaction| interaction.raw_inputs.clone())
            }),
        );
    }

    TelemetryEvent {
        name: "adaptive_card.trace".to_string(),
        properties: Value::Object(properties),
    }
}
