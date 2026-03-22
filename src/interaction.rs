use serde_json::{Map, Value};

use crate::config::RuntimeConfig;
use crate::error::ComponentError;
use crate::model::{
    AdaptiveActionEvent, AdaptiveActionType, AdaptiveCardInvocation, AdaptiveCardResult,
    CardInteractionType, SessionUpdateOp, StateUpdateOp,
};
use crate::render::render_card;
use crate::state_store;
use crate::trace;

pub fn handle_interaction(
    inv: &AdaptiveCardInvocation,
    runtime_config: &RuntimeConfig,
) -> Result<AdaptiveCardResult, ComponentError> {
    let interaction = inv
        .interaction
        .clone()
        .ok_or_else(|| ComponentError::InvalidInput("interaction is required".into()))?;
    if interaction.action_id.trim().is_empty() {
        return Err(ComponentError::InteractionInvalid(
            "interaction.action_id is required".into(),
        ));
    }
    if interaction.card_instance_id.trim().is_empty() {
        return Err(ComponentError::InteractionInvalid(
            "interaction.card_instance_id is required".into(),
        ));
    }

    let mut invocation = inv.clone();
    let state_loaded = state_store::load_state_if_missing(&mut invocation, Some(&interaction))?;
    let state_read_hash = state_loaded.as_ref().and_then(trace::hash_value);
    let resolved = render_card(&invocation, runtime_config)?;
    let normalized_inputs = normalize_inputs(&interaction.raw_inputs);
    let mut state_updates = Vec::new();
    let mut session_updates = Vec::new();

    if let Some(route) = interaction
        .metadata
        .get("route")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        session_updates.push(SessionUpdateOp::SetRoute { route });
    }

    let action_type = match interaction.interaction_type {
        CardInteractionType::Submit => {
            state_updates.push(StateUpdateOp::Merge {
                path: "form_data".into(),
                value: normalized_inputs.clone(),
            });
            AdaptiveActionType::Submit
        }
        CardInteractionType::Execute => {
            state_updates.push(StateUpdateOp::Merge {
                path: "form_data".into(),
                value: normalized_inputs.clone(),
            });
            AdaptiveActionType::Execute
        }
        CardInteractionType::OpenUrl => AdaptiveActionType::OpenUrl,
        CardInteractionType::ShowCard => {
            let subcard_id = interaction
                .metadata
                .get("subcardId")
                .and_then(|v| v.as_str())
                .unwrap_or(&interaction.action_id)
                .to_string();
            state_updates.push(StateUpdateOp::Set {
                path: format!("ui.active_show_card.{}", interaction.card_instance_id),
                value: Value::String(subcard_id.clone()),
            });
            AdaptiveActionType::ShowCard
        }
        CardInteractionType::ToggleVisibility => {
            let visible = interaction
                .metadata
                .get("visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            state_updates.push(StateUpdateOp::Set {
                path: format!("ui.visibility.{}", interaction.action_id),
                value: Value::Bool(visible),
            });
            AdaptiveActionType::ToggleVisibility
        }
    };

    let event = AdaptiveActionEvent {
        action_type,
        action_id: interaction.action_id.clone(),
        verb: interaction.verb.clone(),
        route: interaction
            .metadata
            .get("route")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        inputs: normalized_inputs.clone(),
        card_id: interaction
            .metadata
            .get("cardId")
            .and_then(|v| v.as_str())
            .unwrap_or(&interaction.card_instance_id)
            .to_string(),
        card_instance_id: interaction.card_instance_id.clone(),
        subcard_id: interaction
            .metadata
            .get("subcardId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        metadata: interaction.metadata.clone(),
    };

    let mut persisted_state = if invocation.state.is_null() {
        Value::Object(Map::new())
    } else {
        invocation.state.clone()
    };
    state_store::apply_updates(&mut persisted_state, &state_updates);
    let state_write_hash = trace::hash_value(&persisted_state);
    state_store::persist_state(&invocation, Some(&interaction), &persisted_state)?;

    let mut telemetry_events = Vec::new();
    if runtime_config.trace_enabled {
        let state_key = Some(state_store::state_key_for(&invocation, Some(&interaction)));
        telemetry_events.push(trace::build_trace_event(
            &invocation,
            &resolved.asset_resolution,
            &resolved.binding_summary,
            &trace::TraceContext {
                interaction: Some(interaction.clone()),
                state_key,
                state_read_hash,
                state_write_hash,
                capture_inputs: runtime_config.trace_capture_inputs,
            },
        ));
    }

    Ok(AdaptiveCardResult {
        rendered_card: Some(resolved.card),
        event: Some(event),
        state_updates,
        session_updates,
        card_features: resolved.features,
        validation_issues: resolved.validation_issues,
        telemetry_events,
    })
}

fn normalize_inputs(raw: &Value) -> Value {
    match raw {
        Value::Object(_) => raw.clone(),
        Value::Null => Value::Object(Map::new()),
        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| {
            let mut map = Map::new();
            map.insert("value".into(), Value::String(s.clone()));
            Value::Object(map)
        }),
        other => {
            let mut map = Map::new();
            map.insert("value".into(), other.clone());
            Value::Object(map)
        }
    }
}
