use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::{Map, Value};

use crate::asset_resolver::resolve_with_host;
use crate::error::ComponentError;
use crate::model::{CardSource, ValidationMode};

const LEGACY_ASSET_BASE_ENV: &str = "ADAPTIVE_CARD_ASSET_BASE";
const LEGACY_TRACE_ENV: &str = "GREENTIC_TRACE";
const LEGACY_TRACE_OUT_ENV: &str = "GREENTIC_TRACE_OUT";
const LEGACY_TRACE_CAPTURE_ENV: &str = "GREENTIC_TRACE_CAPTURE_INPUTS";

#[cfg(not(target_arch = "wasm32"))]
const LEGACY_ASSET_REGISTRY_ENV: &str = "ADAPTIVE_CARD_ASSET_REGISTRY";

static SUPPORTED_LOCALES: OnceLock<Vec<String>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum LanguageMode {
    #[default]
    All,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum DirectionMode {
    #[default]
    Ltr,
    Rtl,
    Auto,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentConfigInput {
    pub default_source: Option<CardSource>,
    pub default_card_inline: Option<Value>,
    pub default_card_asset: Option<String>,
    pub catalog_registry_ref: Option<String>,
    pub multilingual: Option<bool>,
    pub language_mode: Option<LanguageMode>,
    pub supported_locales: Option<Vec<String>>,
    pub direction_mode: Option<DirectionMode>,
    pub validation_mode: Option<ValidationMode>,
    pub trace_enabled: Option<bool>,
    pub trace_capture_inputs: Option<bool>,
    pub legacy_asset_base_path: Option<String>,
    pub legacy_catalog_registry_file: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeConfig {
    pub default_source: CardSource,
    pub default_card_inline: Option<Value>,
    pub default_card_asset: Option<String>,
    pub catalog_registry_ref: Option<String>,
    pub multilingual: bool,
    pub language_mode: LanguageMode,
    pub supported_locales: Vec<String>,
    pub direction_mode: DirectionMode,
    pub validation_mode: ValidationMode,
    pub trace_enabled: bool,
    pub trace_capture_inputs: bool,
    pub asset_base_path: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            default_source: CardSource::Inline,
            default_card_inline: None,
            default_card_asset: None,
            catalog_registry_ref: None,
            multilingual: true,
            language_mode: LanguageMode::All,
            supported_locales: supported_locale_codes().to_vec(),
            direction_mode: DirectionMode::Ltr,
            validation_mode: ValidationMode::Warn,
            trace_enabled: false,
            trace_capture_inputs: false,
            asset_base_path: legacy_asset_base_path(None),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DistributionRef {
    Repo { repo: String, path: String },
    Store { reference: String },
}

pub fn supported_locale_codes() -> &'static [String] {
    SUPPORTED_LOCALES.get_or_init(|| {
        serde_json::from_str(include_str!("../config/supported_locales.json"))
            .expect("supported locale list must be valid JSON")
    })
}

pub fn normalize_locale(raw: &str) -> Option<String> {
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

pub fn resolve_locale_against(candidate: &str, allowed: &[String]) -> Option<String> {
    let normalized = normalize_locale(candidate)?;
    for locale in allowed {
        if locale.eq_ignore_ascii_case(&normalized) {
            return Some(locale.clone());
        }
    }
    let base = normalized
        .split('-')
        .next()
        .map(|part| part.to_ascii_lowercase())?;
    for locale in allowed {
        if locale.eq_ignore_ascii_case(&base) {
            return Some(locale.clone());
        }
    }
    None
}

pub fn is_rtl_locale(locale: &str) -> bool {
    matches!(
        normalize_locale(locale).as_deref(),
        Some("ar")
            | Some("ar-AE")
            | Some("ar-DZ")
            | Some("ar-EG")
            | Some("ar-IQ")
            | Some("ar-MA")
            | Some("ar-SA")
            | Some("ar-SD")
            | Some("ar-SY")
            | Some("ar-TN")
    )
}

impl RuntimeConfig {
    pub fn resolve_locale(&self, candidate: &str) -> Option<String> {
        if !self.multilingual {
            return Some("en".to_string());
        }
        resolve_locale_against(candidate, &self.supported_locales)
    }

    pub fn effective_direction_rtl(&self, locale: &str) -> bool {
        match self.direction_mode {
            DirectionMode::Ltr => false,
            DirectionMode::Rtl => true,
            DirectionMode::Auto => is_rtl_locale(locale),
        }
    }
}

pub fn resolve_runtime_config(component_config: Option<&ComponentConfigInput>) -> RuntimeConfig {
    let mut resolved = RuntimeConfig::default();

    if let Some(config) = component_config {
        if let Some(source) = config.default_source.clone() {
            resolved.default_source = source;
        }
        if let Some(inline) = config.default_card_inline.clone() {
            resolved.default_card_inline = Some(inline);
        }
        if let Some(asset) = trimmed_string(config.default_card_asset.as_deref()) {
            resolved.default_card_asset = Some(asset);
        }
        if let Some(registry_ref) = trimmed_string(config.catalog_registry_ref.as_deref()) {
            resolved.catalog_registry_ref = Some(registry_ref);
        } else if let Some(legacy_file) =
            trimmed_string(config.legacy_catalog_registry_file.as_deref())
        {
            resolved.catalog_registry_ref = Some(legacy_file);
        }
        if let Some(multilingual) = config.multilingual {
            resolved.multilingual = multilingual;
        }
        if let Some(language_mode) = config.language_mode.clone() {
            resolved.language_mode = language_mode;
        }
        if let Some(direction_mode) = config.direction_mode.clone() {
            resolved.direction_mode = direction_mode;
        }
        if let Some(validation_mode) = config.validation_mode.clone() {
            resolved.validation_mode = validation_mode;
        }
        if let Some(trace_enabled) = config.trace_enabled {
            resolved.trace_enabled = trace_enabled;
        } else {
            resolved.trace_enabled = legacy_trace_enabled();
        }
        if let Some(capture) = config.trace_capture_inputs {
            resolved.trace_capture_inputs = capture;
        } else {
            resolved.trace_capture_inputs = legacy_trace_capture_inputs();
        }
        resolved.asset_base_path = legacy_asset_base_path(config.legacy_asset_base_path.as_deref());
    } else {
        resolved.trace_enabled = legacy_trace_enabled();
        resolved.trace_capture_inputs = legacy_trace_capture_inputs();
        resolved.asset_base_path = legacy_asset_base_path(None);
    }

    resolved.supported_locales = match (resolved.multilingual, &resolved.language_mode) {
        (false, _) => vec!["en".to_string()],
        (true, LanguageMode::All) => supported_locale_codes().to_vec(),
        (true, LanguageMode::Custom) => {
            let mut locales = config_supported_locales(component_config);
            if !locales
                .iter()
                .any(|locale| locale.eq_ignore_ascii_case("en"))
            {
                locales.insert(0, "en".to_string());
            }
            if locales.is_empty() {
                vec!["en".to_string()]
            } else {
                locales
            }
        }
    };

    resolved
}

pub fn parse_component_config_from_value(value: &Value) -> Option<ComponentConfigInput> {
    for candidate in component_config_candidates(value) {
        if looks_like_component_config(candidate) {
            return Some(parse_component_config_object(candidate));
        }
    }
    None
}

pub fn env_asset_registry() -> Result<Option<BTreeMap<String, String>>, ComponentError> {
    #[cfg(target_arch = "wasm32")]
    {
        Ok(None)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let file = match std::env::var(LEGACY_ASSET_REGISTRY_ENV) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        load_registry_map(&file).map(Some)
    }
}

pub fn load_registry_map(ref_or_path: &str) -> Result<BTreeMap<String, String>, ComponentError> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ref_or_path;
        Err(ComponentError::Asset(
            "catalog registry refs must be resolved by the host in wasm mode".to_string(),
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let resolved = resolve_reference_path(ref_or_path)?;
        let content = std::fs::read_to_string(&resolved)?;
        let map: BTreeMap<String, String> = serde_json::from_str(&content)?;
        Ok(map)
    }
}

pub fn resolve_reference_path(ref_or_path: &str) -> Result<String, ComponentError> {
    if let Some(reference) = parse_distribution_ref(ref_or_path) {
        return resolve_distribution_ref(&reference);
    }
    Ok(ref_or_path.to_string())
}

pub fn parse_distribution_ref(raw: &str) -> Option<DistributionRef> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("repo://") {
        let (repo, path) = rest.split_once('/')?;
        if repo.is_empty() || path.trim().is_empty() {
            return None;
        }
        return Some(DistributionRef::Repo {
            repo: repo.to_string(),
            path: path.to_string(),
        });
    }
    if trimmed.starts_with("store://") {
        return Some(DistributionRef::Store {
            reference: trimmed.to_string(),
        });
    }
    None
}

fn resolve_distribution_ref(reference: &DistributionRef) -> Result<String, ComponentError> {
    match reference {
        DistributionRef::Repo { path, .. } => Ok(PathBuf::from(path).to_string_lossy().to_string()),
        DistributionRef::Store { reference } => {
            if let Some(path) =
                resolve_with_host(reference).map_err(|err| ComponentError::Asset(err.message))?
            {
                return Ok(path);
            }
            Err(ComponentError::Asset(format!(
                "store ref requires host/distributor resolution: {reference}"
            )))
        }
    }
}

fn parse_component_config_object(object: &Map<String, Value>) -> ComponentConfigInput {
    ComponentConfigInput {
        default_source: object
            .get("default_source")
            .or_else(|| object.get("card_source"))
            .and_then(parse_card_source),
        default_card_inline: object
            .get("default_card_inline")
            .cloned()
            .filter(|value| value.is_object() || value.is_array()),
        default_card_asset: string_value(object.get("default_card_asset")),
        catalog_registry_ref: string_value(object.get("catalog_registry_ref")),
        multilingual: object.get("multilingual").and_then(Value::as_bool),
        language_mode: object
            .get("language_mode")
            .and_then(Value::as_str)
            .and_then(parse_language_mode),
        supported_locales: parse_supported_locales(object.get("supported_locales")),
        direction_mode: object
            .get("direction_mode")
            .and_then(Value::as_str)
            .and_then(parse_direction_mode),
        validation_mode: object
            .get("validation_mode")
            .and_then(parse_validation_mode),
        trace_enabled: object.get("trace_enabled").and_then(Value::as_bool),
        trace_capture_inputs: object.get("trace_capture_inputs").and_then(Value::as_bool),
        legacy_asset_base_path: string_value(object.get("asset_base_path")),
        legacy_catalog_registry_file: string_value(object.get("catalog_registry_file")),
    }
}

fn component_config_candidates(value: &Value) -> Vec<&Map<String, Value>> {
    let mut candidates = Vec::new();
    if let Some(object) = value.as_object() {
        candidates.push(object);
        if let Some(config) = object.get("config").and_then(Value::as_object) {
            candidates.push(config);
        }
        if let Some(tool) = object.get("tool").and_then(Value::as_object) {
            candidates.push(tool);
        }
        if let Some(node) = object.get("node").and_then(Value::as_object)
            && let Some(tool) = node.get("tool").and_then(Value::as_object)
        {
            candidates.push(tool);
        }
    }
    candidates
}

fn looks_like_component_config(object: &Map<String, Value>) -> bool {
    [
        "default_source",
        "default_card_inline",
        "default_card_asset",
        "catalog_registry_ref",
        "multilingual",
        "language_mode",
        "supported_locales",
        "direction_mode",
        "validation_mode",
        "trace_enabled",
        "trace_capture_inputs",
        "asset_base_path",
        "catalog_registry_file",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
}

fn parse_card_source(value: &Value) -> Option<CardSource> {
    let raw = value.as_str()?.to_ascii_lowercase();
    match raw.as_str() {
        "inline" => Some(CardSource::Inline),
        "asset" => Some(CardSource::Asset),
        "catalog" => Some(CardSource::Catalog),
        _ => None,
    }
}

fn parse_language_mode(raw: &str) -> Option<LanguageMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "all" => Some(LanguageMode::All),
        "custom" => Some(LanguageMode::Custom),
        _ => None,
    }
}

fn parse_direction_mode(raw: &str) -> Option<DirectionMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "ltr" => Some(DirectionMode::Ltr),
        "rtl" => Some(DirectionMode::Rtl),
        "auto" => Some(DirectionMode::Auto),
        _ => None,
    }
}

fn parse_supported_locales(value: Option<&Value>) -> Option<Vec<String>> {
    let raw = value?;
    let locales = if let Some(array) = raw.as_array() {
        array.iter().filter_map(Value::as_str).collect::<Vec<_>>()
    } else if let Some(text) = raw.as_str() {
        text.split(',').collect::<Vec<_>>()
    } else {
        return None;
    };

    let mut resolved = Vec::new();
    for locale in locales {
        if let Some(normalized) = resolve_locale_against(locale, supported_locale_codes())
            && !resolved
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&normalized))
        {
            resolved.push(normalized);
        }
    }
    Some(resolved)
}

fn config_supported_locales(component_config: Option<&ComponentConfigInput>) -> Vec<String> {
    component_config
        .and_then(|config| config.supported_locales.clone())
        .unwrap_or_else(|| vec!["en".to_string()])
}

fn parse_validation_mode(value: &Value) -> Option<ValidationMode> {
    let raw = value.as_str()?.to_ascii_lowercase();
    match raw.as_str() {
        "off" => Some(ValidationMode::Off),
        "warn" => Some(ValidationMode::Warn),
        "error" => Some(ValidationMode::Error),
        _ => None,
    }
}

fn string_value(value: Option<&Value>) -> Option<String> {
    trimmed_string(value.and_then(Value::as_str))
}

fn trimmed_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn legacy_asset_base_path(config_value: Option<&str>) -> String {
    trimmed_string(config_value)
        .or_else(|| std::env::var(LEGACY_ASSET_BASE_ENV).ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "assets".to_string())
}

fn legacy_trace_enabled() -> bool {
    std::env::var(LEGACY_TRACE_OUT_ENV).is_ok()
        || std::env::var(LEGACY_TRACE_ENV)
            .map(|value| value == "1")
            .unwrap_or(false)
}

fn legacy_trace_capture_inputs() -> bool {
    std::env::var(LEGACY_TRACE_CAPTURE_ENV)
        .map(|value| value == "1")
        .unwrap_or(false)
}
