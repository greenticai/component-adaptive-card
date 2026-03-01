use std::sync::OnceLock;

use serde_json::Value;

use crate::i18n_bundle::{LocaleBundle, unpack_locales_from_cbor};
use crate::model::AdaptiveCardInvocation;

include!(concat!(env!("OUT_DIR"), "/i18n_bundle.rs"));

static I18N_BUNDLE: OnceLock<LocaleBundle> = OnceLock::new();

fn bundle() -> &'static LocaleBundle {
    I18N_BUNDLE.get_or_init(|| unpack_locales_from_cbor(I18N_BUNDLE_CBOR).unwrap_or_default())
}

fn normalize_locale(raw: &str) -> Option<String> {
    let mut cleaned = raw.trim();
    if cleaned.is_empty() {
        return None;
    }
    if let Some((head, _)) = cleaned.split_once('.') {
        cleaned = head;
    }
    if let Some((head, _)) = cleaned.split_once('@') {
        cleaned = head;
    }
    let cleaned = cleaned.replace('_', "-");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn resolve_supported_locale(candidate: &str) -> Option<String> {
    let normalized = normalize_locale(candidate)?;
    for locale in bundle().keys() {
        if locale.eq_ignore_ascii_case(&normalized) {
            return Some(locale.clone());
        }
    }
    let base = normalized
        .split('-')
        .next()
        .map(|s| s.to_ascii_lowercase())?;
    for locale in bundle().keys() {
        if locale.eq_ignore_ascii_case(&base) {
            return Some(locale.clone());
        }
    }
    None
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

pub fn resolve_locale(inv: &AdaptiveCardInvocation) -> String {
    let from_invocation = inv.locale.as_deref();
    let from_session = extract_locale_from_session(&inv.session);
    let from_envelope = extract_locale_from_envelope(inv);

    if let Some(locale) = from_invocation.and_then(resolve_supported_locale) {
        return locale;
    }
    if let Some(locale) = from_session.as_deref().and_then(resolve_supported_locale) {
        return locale;
    }
    if let Some(locale) = from_envelope.as_deref().and_then(resolve_supported_locale) {
        return locale;
    }
    "en".to_string()
}

pub fn resolve_locale_from_raw(value: &Value) -> String {
    if let Some(locale) = value.get("locale").and_then(Value::as_str)
        && let Some(supported) = resolve_supported_locale(locale)
    {
        return supported;
    }
    if let Some(locale) = value.get("i18n_locale").and_then(Value::as_str)
        && let Some(supported) = resolve_supported_locale(locale)
    {
        return supported;
    }
    if let Some(session) = value.get("session")
        && let Some(locale) = extract_locale_from_session(session)
        && let Some(supported) = resolve_supported_locale(&locale)
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
    for candidate in locale_chain(locale) {
        if let Some(map) = bundle().get(&candidate)
            && let Some(value) = map.get(key)
        {
            return format_message(value.clone(), args);
        }
    }
    key.to_string()
}
