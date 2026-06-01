use serde_json::{Map, Value};

use crate::error::ComponentError;
use crate::model::{AdaptiveCardInvocation, CardInteraction, StateUpdateOp};

#[cfg(all(target_arch = "wasm32", feature = "state-store"))]
use greentic_interfaces_guest::state_store;

#[cfg(not(target_arch = "wasm32"))]
use once_cell::sync::Lazy;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

#[cfg(not(target_arch = "wasm32"))]
static STATE_STORE: Lazy<Mutex<HashMap<String, Vec<u8>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn load_state_if_missing(
    inv: &mut AdaptiveCardInvocation,
    interaction: Option<&CardInteraction>,
) -> Result<Option<Value>, ComponentError> {
    if !inv.state.is_null() {
        return Ok(None);
    }
    let key = state_key(inv, interaction);
    let loaded = read_state(&key)?;
    if let Some(state) = loaded.clone() {
        inv.state = state;
    }
    Ok(loaded)
}

pub fn persist_state(
    inv: &AdaptiveCardInvocation,
    interaction: Option<&CardInteraction>,
    state: &Value,
) -> Result<(), ComponentError> {
    let key = state_key(inv, interaction);
    if state.is_null() {
        delete_state(&key)?;
        return Ok(());
    }
    let bytes = serde_json::to_vec(state)?;
    write_state(&key, bytes)
}

pub fn state_key_for(
    inv: &AdaptiveCardInvocation,
    interaction: Option<&CardInteraction>,
) -> String {
    state_key(inv, interaction)
}

pub fn apply_updates(state: &mut Value, updates: &[StateUpdateOp]) {
    for update in updates {
        match update {
            StateUpdateOp::Set { path, value } => set_path(state, path, value.clone()),
            StateUpdateOp::Merge { path, value } => merge_path(state, path, value.clone()),
            StateUpdateOp::Delete { path } => delete_path(state, path),
        }
    }
}

fn state_key(inv: &AdaptiveCardInvocation, interaction: Option<&CardInteraction>) -> String {
    if let Some(node_id) = inv.node_id.as_deref() {
        return format!("adaptive-card:node:{node_id}");
    }
    if let Some(interaction) = interaction {
        return format!("adaptive-card:card:{}", interaction.card_instance_id);
    }
    "adaptive-card:default".to_string()
}

fn set_path(state: &mut Value, path: &str, value: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        *state = value;
        return;
    }
    let mut current = state;
    for part in &parts[..parts.len().saturating_sub(1)] {
        ensure_object(current);
        if let Value::Object(map) = current {
            if !map.contains_key(*part) {
                map.insert((*part).to_string(), Value::Object(Map::new()));
            }
            let next = map.get_mut(*part).expect("just inserted");
            current = next;
        }
    }
    ensure_object(current);
    if let Value::Object(map) = current {
        map.insert(parts[parts.len() - 1].to_string(), value);
    }
}

fn merge_path(state: &mut Value, path: &str, value: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        *state = value;
        return;
    }
    let mut current = state;
    for part in &parts[..parts.len().saturating_sub(1)] {
        ensure_object(current);
        if let Value::Object(map) = current {
            if !map.contains_key(*part) {
                map.insert((*part).to_string(), Value::Object(Map::new()));
            }
            let next = map.get_mut(*part).expect("just inserted");
            current = next;
        }
    }
    ensure_object(current);
    if let Value::Object(map) = current {
        let key = parts[parts.len() - 1];
        match (map.get_mut(key), value) {
            (Some(Value::Object(existing)), Value::Object(update)) => {
                for (k, v) in update {
                    existing.insert(k, v);
                }
            }
            (_, other) => {
                map.insert(key.to_string(), other);
            }
        }
    }
}

fn delete_path(state: &mut Value, path: &str) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        *state = Value::Null;
        return;
    }
    let mut current = state;
    for part in &parts[..parts.len().saturating_sub(1)] {
        match current {
            Value::Object(map) => {
                current = match map.get_mut(*part) {
                    Some(value) => value,
                    None => return,
                };
            }
            _ => return,
        }
    }
    if let Value::Object(map) = current {
        map.remove(parts[parts.len() - 1]);
    }
}

fn ensure_object(value: &mut Value) {
    if !matches!(value, Value::Object(_)) {
        *value = Value::Object(Map::new());
    }
}

fn read_state(key: &str) -> Result<Option<Value>, ComponentError> {
    let bytes = read_bytes(key)?;
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    if bytes.is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    Ok(Some(value))
}

#[cfg(all(target_arch = "wasm32", feature = "state-store"))]
fn read_bytes(key: &str) -> Result<Option<Vec<u8>>, ComponentError> {
    match state_store::read(key, None) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if is_not_found(&err.code) => Ok(None),
        Err(err) => Err(ComponentError::StateStore(format!(
            "read failed: {} ({})",
            err.message, err.code
        ))),
    }
}

#[cfg(all(target_arch = "wasm32", not(feature = "state-store")))]
fn read_bytes(_key: &str) -> Result<Option<Vec<u8>>, ComponentError> {
    Ok(None)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_bytes(key: &str) -> Result<Option<Vec<u8>>, ComponentError> {
    let store = STATE_STORE
        .lock()
        .map_err(|_| ComponentError::StateStore("state store poisoned".into()))?;
    Ok(store.get(key).cloned())
}

#[cfg(all(target_arch = "wasm32", feature = "state-store"))]
fn write_state(key: &str, bytes: Vec<u8>) -> Result<(), ComponentError> {
    match state_store::write(key, &bytes, None) {
        Ok(state_store::OpAck::Ok) => Ok(()),
        Err(err) => Err(ComponentError::StateStore(format!(
            "write failed: {} ({})",
            err.message, err.code
        ))),
    }
}

#[cfg(all(target_arch = "wasm32", not(feature = "state-store")))]
fn write_state(_key: &str, _bytes: Vec<u8>) -> Result<(), ComponentError> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_state(key: &str, bytes: Vec<u8>) -> Result<(), ComponentError> {
    let mut store = STATE_STORE
        .lock()
        .map_err(|_| ComponentError::StateStore("state store poisoned".into()))?;
    store.insert(key.to_string(), bytes);
    Ok(())
}

#[cfg(all(target_arch = "wasm32", feature = "state-store"))]
fn delete_state(key: &str) -> Result<(), ComponentError> {
    match state_store::delete(key, None) {
        Ok(state_store::OpAck::Ok) => Ok(()),
        Err(err) => Err(ComponentError::StateStore(format!(
            "delete failed: {} ({})",
            err.message, err.code
        ))),
    }
}

#[cfg(all(target_arch = "wasm32", not(feature = "state-store")))]
fn delete_state(_key: &str) -> Result<(), ComponentError> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn delete_state(key: &str) -> Result<(), ComponentError> {
    let mut store = STATE_STORE
        .lock()
        .map_err(|_| ComponentError::StateStore("state store poisoned".into()))?;
    store.remove(key);
    Ok(())
}

#[cfg(all(target_arch = "wasm32", feature = "state-store"))]
fn is_not_found(code: &str) -> bool {
    let normalized = code.to_ascii_lowercase();
    normalized == "not-found"
        || normalized == "not_found"
        || normalized == "notfound"
        || normalized == "state.read.miss"
        || normalized.contains("read.miss")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        AdaptiveCardInvocation, CardSource, CardSpec, InvocationMode, ValidationMode,
    };
    use serde_json::json;

    fn base_invocation() -> AdaptiveCardInvocation {
        AdaptiveCardInvocation {
            card_source: CardSource::Inline,
            card_spec: CardSpec {
                inline_json: Some(json!({})),
                ..Default::default()
            },
            node_id: Some("node-1".to_string()),
            locale: None,
            payload: Value::Null,
            session: Value::Null,
            state: Value::Null,
            interaction: None,
            mode: InvocationMode::RenderAndValidate,
            validation_mode: ValidationMode::Warn,
            prefill: None,
            envelope: None,
        }
    }

    #[test]
    fn apply_updates_sets_merges_and_deletes() {
        let mut state = Value::Object(Map::new());
        let updates = vec![
            StateUpdateOp::Set {
                path: "form_data.name".into(),
                value: Value::String("Ada".into()),
            },
            StateUpdateOp::Merge {
                path: "form_data".into(),
                value: json!({"tier": "pro"}),
            },
            StateUpdateOp::Delete {
                path: "form_data.name".into(),
            },
        ];
        apply_updates(&mut state, &updates);
        assert_eq!(state["form_data"]["tier"], "pro");
        assert!(state["form_data"]["name"].is_null());
    }

    #[test]
    fn persists_and_loads_state_when_missing() {
        let mut invocation = base_invocation();
        let state = json!({"ui": {"visibility": {"card": true}}});
        persist_state(&invocation, None, &state).expect("persist should succeed");

        let loaded = load_state_if_missing(&mut invocation, None).expect("load should succeed");
        assert_eq!(loaded, Some(state));
        assert_eq!(invocation.state["ui"]["visibility"]["card"], true);
    }
}
