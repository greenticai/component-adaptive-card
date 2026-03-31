use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};

use serde_json::Value;

use crate::config::{RuntimeConfig, resolve_locale_against, supported_locale_codes};
use crate::i18n_bundle::{LocaleBundle, unpack_locales_from_cbor};
use crate::model::AdaptiveCardInvocation;

include!(concat!(env!("OUT_DIR"), "/i18n_bundle.rs"));

static I18N_BUNDLE: OnceLock<LocaleBundle> = OnceLock::new();
static EXTERNAL_BUNDLE: RwLock<LocaleBundle> = RwLock::new(BTreeMap::new());

fn bundle() -> &'static LocaleBundle {
    I18N_BUNDLE.get_or_init(|| unpack_locales_from_cbor(I18N_BUNDLE_CBOR).unwrap_or_default())
}

/// Load an external i18n bundle from a JSON string.
///
/// The JSON must be a flat `{ "key": "value" }` map. Entries are merged into
/// the external bundle under the given `locale` code so that subsequent `t()`
/// and `tf()` calls resolve them before falling back to the compiled-in bundle.
pub fn load_external_locale(locale: &str, json_str: &str) -> Result<(), String> {
    let map: BTreeMap<String, String> =
        serde_json::from_str(json_str).map_err(|err| err.to_string())?;
    if let Ok(mut ext) = EXTERNAL_BUNDLE.write() {
        ext.entry(locale.to_string()).or_default().extend(map);
        Ok(())
    } else {
        Err("external i18n bundle lock poisoned".to_string())
    }
}

/// Clear all previously loaded external i18n entries.
pub fn clear_external_bundle() {
    if let Ok(mut ext) = EXTERNAL_BUNDLE.write() {
        ext.clear();
    }
}

fn resolve_supported_locale(candidate: &str) -> Option<String> {
    resolve_locale_against(candidate, supported_locale_codes())
}

fn extract_locale_from_session(session: &Value) -> Option<String> {
    if let Some(locale) = session.get("locale").and_then(Value::as_str) {
        return Some(locale.to_string());
    }
    if let Some(locale) = session
        .get("i18n")
        .and_then(|v| v.get("locale"))
        .and_then(Value::as_str)
    {
        return Some(locale.to_string());
    }
    None
}

fn extract_locale_from_envelope(inv: &AdaptiveCardInvocation) -> Option<String> {
    if let Some(envelope) = inv.envelope.as_ref() {
        let i18n_id = envelope.ctx.i18n_id.trim();
        if !i18n_id.is_empty() {
            return Some(i18n_id.to_string());
        }
        if let Some(metadata) = envelope.metadata_cbor.as_ref()
            && let Ok(value) = greentic_types::cbor::canonical::from_cbor::<Value>(metadata)
        {
            if let Some(locale) = value.get("locale").and_then(Value::as_str) {
                return Some(locale.to_string());
            }
            if let Some(locale) = value
                .get("i18n")
                .and_then(|v| v.get("locale"))
                .and_then(Value::as_str)
            {
                return Some(locale.to_string());
            }
        }
    }
    None
}

pub fn resolve_locale_with_config(
    inv: &AdaptiveCardInvocation,
    runtime_config: &RuntimeConfig,
) -> String {
    if !runtime_config.multilingual {
        return "en".to_string();
    }
    let from_invocation = inv.locale.as_deref();
    let from_session = extract_locale_from_session(&inv.session);
    let from_envelope = extract_locale_from_envelope(inv);

    if let Some(locale) = from_invocation.and_then(|value| runtime_config.resolve_locale(value)) {
        return locale;
    }
    if let Some(locale) = from_session
        .as_deref()
        .and_then(|value| runtime_config.resolve_locale(value))
    {
        return locale;
    }
    if let Some(locale) = from_envelope
        .as_deref()
        .and_then(|value| runtime_config.resolve_locale(value))
    {
        return locale;
    }
    "en".to_string()
}

pub fn resolve_locale_from_raw_with_config(
    value: &Value,
    runtime_config: &RuntimeConfig,
) -> String {
    if !runtime_config.multilingual {
        return "en".to_string();
    }
    if let Some(locale) = value.get("locale").and_then(Value::as_str)
        && let Some(supported) = runtime_config.resolve_locale(locale)
    {
        return supported;
    }
    if let Some(locale) = value.get("i18n_locale").and_then(Value::as_str)
        && let Some(supported) = runtime_config.resolve_locale(locale)
    {
        return supported;
    }
    if let Some(session) = value.get("session")
        && let Some(locale) = extract_locale_from_session(session)
        && let Some(supported) = runtime_config.resolve_locale(&locale)
    {
        return supported;
    }
    "en".to_string()
}

fn locale_chain(locale: &str) -> Vec<String> {
    let resolved = resolve_supported_locale(locale).unwrap_or_else(|| "en".to_string());
    let base = resolved
        .split('-')
        .next()
        .unwrap_or("en")
        .to_ascii_lowercase();
    let mut chain = vec![resolved.clone()];
    if !resolved.eq_ignore_ascii_case(&base) {
        chain.push(base);
    }
    if !chain.iter().any(|entry| entry.eq_ignore_ascii_case("en")) {
        chain.push("en".to_string());
    }
    chain
}

fn format_message(mut message: String, args: &[(&str, &str)]) -> String {
    for (key, value) in args {
        let needle = format!("{{{key}}}");
        message = message.replace(&needle, value);
    }
    message
}

pub fn t(locale: &str, key: &str) -> String {
    tf(locale, key, &[])
}

pub fn tf(locale: &str, key: &str, args: &[(&str, &str)]) -> String {
    let chain = locale_chain(locale);

    // Check external (pack) bundle first so custom keys override compiled-in ones.
    if let Ok(ext) = EXTERNAL_BUNDLE.read() {
        for candidate in &chain {
            if let Some(map) = ext.get(candidate)
                && let Some(value) = map.get(key)
            {
                return format_message(value.clone(), args);
            }
        }
    }

    // Fall back to the compiled-in bundle.
    for candidate in &chain {
        if let Some(map) = bundle().get(candidate)
            && let Some(value) = map.get(key)
        {
            return format_message(value.clone(), args);
        }
    }
    key.to_string()
}
