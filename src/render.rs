use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use handlebars::Handlebars;
use serde_json::{Map, Value};

use crate::asset_resolver::resolve_with_host;
use crate::config::{RuntimeConfig, env_asset_registry, load_registry_map, resolve_reference_path};
use crate::error::ComponentError;
use crate::expression::{ExpressionEngine, SimpleExpressionEngine, stringify_value};
use crate::i18n;
use crate::model::{
    AdaptiveCardInvocation, CardFeatureSummary, CardSource, CardSpec, ValidationIssue,
};

#[derive(Debug, Default, Clone)]
pub struct BindingSummary {
    pub handlebars_expansions: u64,
    pub placeholder_replacements: u64,
    pub expression_evaluations: u64,
    pub missing_paths: u64,
}

#[derive(Debug, Default, Clone)]
pub struct AssetResolution {
    pub mode: String,
    pub resolved: Option<String>,
    pub hash: Option<String>,
}

#[derive(Debug)]
pub struct RenderOutcome {
    pub card: Value,
    pub features: CardFeatureSummary,
    pub validation_issues: Vec<ValidationIssue>,
    pub asset_resolution: AssetResolution,
    pub binding_summary: BindingSummary,
}

pub fn render_card(
    inv: &AdaptiveCardInvocation,
    runtime_config: &RuntimeConfig,
) -> Result<RenderOutcome, ComponentError> {
    let locale = i18n::resolve_locale_with_config(inv, runtime_config);
    let mut summary = BindingSummary::default();
    let (mut card, asset_resolution) = resolve_card(inv, runtime_config)?;
    load_external_i18n_bundle(inv, &locale);
    apply_i18n_markers(&mut card, &locale);
    apply_root_locale_metadata(&mut card, &locale, runtime_config);
    apply_handlebars(&mut card, inv, &mut summary)?;
    let ctx = BindingContext::from_invocation(inv);
    let engine = SimpleExpressionEngine;
    apply_bindings(&mut card, &ctx, &engine, &mut summary)?;

    let features = analyze_features(&card);
    let validation_issues = validate_card(&card, &locale);

    Ok(RenderOutcome {
        card,
        features,
        validation_issues,
        asset_resolution,
        binding_summary: summary,
    })
}

fn resolve_card(
    inv: &AdaptiveCardInvocation,
    runtime_config: &RuntimeConfig,
) -> Result<(Value, AssetResolution), ComponentError> {
    match inv.card_source {
        CardSource::Inline => {
            let card =
                inv.card_spec.inline_json.clone().ok_or_else(|| {
                    ComponentError::InvalidInput("inline_json is required".into())
                })?;
            let hash = hash_json(&card);
            Ok((
                card,
                AssetResolution {
                    mode: "inline".to_string(),
                    resolved: None,
                    hash,
                },
            ))
        }
        CardSource::Asset => {
            let path = inv
                .card_spec
                .asset_path
                .as_ref()
                .ok_or_else(|| ComponentError::InvalidInput("asset_path is required".into()))?;
            let candidates =
                candidate_asset_paths(path, inv.card_spec.asset_registry.as_ref(), runtime_config)?;
            load_with_candidates(path, candidates)
        }
        CardSource::Catalog => {
            let catalog =
                inv.card_spec.catalog_name.as_ref().ok_or_else(|| {
                    ComponentError::InvalidInput("catalog_name is required".into())
                })?;
            let normalized = catalog.trim_start_matches('/');
            let candidates = candidate_catalog_paths(normalized, &inv.card_spec, runtime_config)?;
            load_with_candidates(normalized, candidates)
        }
    }
}

fn resolve_catalog_mapping(
    name: &str,
    spec: &CardSpec,
    runtime_config: &RuntimeConfig,
) -> Result<Option<String>, ComponentError> {
    if let Some(registry) = spec.asset_registry.as_ref()
        && let Some(path) = registry.get(name)
    {
        return Ok(Some(path.to_string()));
    }
    if let Some(registry_ref) = runtime_config.catalog_registry_ref.as_deref() {
        let map = load_registry_map(registry_ref)?;
        if let Some(path) = map.get(name) {
            return Ok(Some(path.to_string()));
        }
    }
    if let Some(env_registry) = env_asset_registry()?
        && let Some(path) = env_registry.get(name)
    {
        return Ok(Some(path.to_string()));
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = name;
        Ok(None)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let file = match std::env::var("ADAPTIVE_CARD_CATALOG_FILE") {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let content = std::fs::read_to_string(file)?;
        let map: BTreeMap<String, String> = serde_json::from_str(&content)?;
        Ok(map.get(name).cloned())
    }
}

fn candidate_asset_paths(
    path: &str,
    registry: Option<&BTreeMap<String, String>>,
    runtime_config: &RuntimeConfig,
) -> Result<Vec<String>, ComponentError> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let push = |value: String, seen: &mut HashSet<String>, list: &mut Vec<String>| {
        if seen.insert(value.clone()) {
            list.push(value);
        }
    };

    if let Some(registry) = registry
        && let Some(mapped) = registry.get(path)
    {
        push(mapped.to_string(), &mut seen, &mut candidates);
    }

    if let Ok(Some(env_map)) = env_asset_registry()
        && let Some(mapped) = env_map.get(path)
    {
        push(mapped.to_string(), &mut seen, &mut candidates);
    }

    if Path::new(path).is_absolute()
        || path.starts_with("./")
        || path.starts_with("../")
        || path.contains('/')
    {
        push(path.to_string(), &mut seen, &mut candidates);
    } else {
        let base = runtime_config.asset_base_path.clone();
        let joined = PathBuf::from(base).join(path).to_string_lossy().to_string();
        push(joined, &mut seen, &mut candidates);
        push(path.to_string(), &mut seen, &mut candidates);
    }

    Ok(candidates)
}

fn candidate_catalog_paths(
    name: &str,
    spec: &CardSpec,
    runtime_config: &RuntimeConfig,
) -> Result<Vec<String>, ComponentError> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let push = |value: String, seen: &mut HashSet<String>, list: &mut Vec<String>| {
        if seen.insert(value.clone()) {
            list.push(value);
        }
    };

    if let Some(mapped) = resolve_catalog_mapping(name, spec, runtime_config)? {
        push(mapped, &mut seen, &mut candidates);
    }

    let base = runtime_config.asset_base_path.clone();
    let path = format!("{}/{}.json", base, name);
    push(path, &mut seen, &mut candidates);
    if Path::new(name).is_absolute() || name.contains('/') || name.ends_with(".json") {
        push(name.to_string(), &mut seen, &mut candidates);
    }

    Ok(candidates)
}

fn load_card_from_path(path: &str) -> Result<(Value, String), ComponentError> {
    let resolved_path = resolve_reference_path(path)?;
    let content = std::fs::read_to_string(&resolved_path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            ComponentError::AssetNotFound(path.to_string())
        } else {
            ComponentError::Io(err)
        }
    })?;
    let json: Value = serde_json::from_str(&content)
        .map_err(|err| ComponentError::AssetParse(format!("{resolved_path}: {err}")))?;
    let hash = hash_bytes(content.as_bytes());
    Ok((json, hash))
}

fn apply_root_locale_metadata(card: &mut Value, locale: &str, runtime_config: &RuntimeConfig) {
    let Some(map) = card.as_object_mut() else {
        return;
    };
    map.insert("lang".to_string(), Value::String(locale.to_string()));
    map.insert(
        "rtl".to_string(),
        Value::Bool(runtime_config.effective_direction_rtl(locale)),
    );
}

fn load_with_candidates(
    lookup_key: &str,
    candidates: Vec<String>,
) -> Result<(Value, AssetResolution), ComponentError> {
    let mut last_err: Option<ComponentError> = None;
    for candidate in candidates {
        match load_card_from_path(&candidate) {
            Ok((card, hash)) => {
                return Ok((
                    card,
                    AssetResolution {
                        mode: "wasm".to_string(),
                        resolved: Some(candidate),
                        hash: Some(hash),
                    },
                ));
            }
            Err(err) => last_err = Some(err),
        }
    }

    if let Some(host) =
        resolve_with_host(lookup_key).map_err(|e| ComponentError::Asset(e.message))?
    {
        match load_card_from_path(&host) {
            Ok((card, hash)) => {
                return Ok((
                    card,
                    AssetResolution {
                        mode: "host".to_string(),
                        resolved: Some(host),
                        hash: Some(hash),
                    },
                ));
            }
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        ComponentError::InvalidInput(format!("unable to resolve card for {lookup_key}"))
    }))
}

#[derive(Debug)]
pub struct BindingContext {
    payload: Value,
    session: Value,
    state: Value,
    template_params: Value,
}

impl BindingContext {
    fn from_invocation(inv: &AdaptiveCardInvocation) -> Self {
        BindingContext {
            payload: inv.payload.clone(),
            session: inv.session.clone(),
            state: inv.state.clone(),
            template_params: inv
                .card_spec
                .template_params
                .clone()
                .unwrap_or(Value::Object(Map::new())),
        }
    }

    pub fn lookup(&self, raw: &str) -> Option<Value> {
        let (path, default) = parse_binding_path(raw);
        let mut segments = path.split('.');
        let first = segments.next()?;
        let attempt_root = |root: &Value, rest: std::str::Split<'_, char>| lookup_in(root, rest);

        let found = match first {
            "payload" => attempt_root(&self.payload, segments),
            "session" => attempt_root(&self.session, segments),
            "state" => attempt_root(&self.state, segments),
            "params" | "template" => attempt_root(&self.template_params, segments),
            _ => lookup_in(
                &self.payload,
                normalize_path(&path)
                    .split('.')
                    .collect::<Vec<_>>()
                    .into_iter(),
            )
            .or_else(|| {
                lookup_in(
                    &self.session,
                    normalize_path(&path)
                        .split('.')
                        .collect::<Vec<_>>()
                        .into_iter(),
                )
            })
            .or_else(|| {
                lookup_in(
                    &self.state,
                    normalize_path(&path)
                        .split('.')
                        .collect::<Vec<_>>()
                        .into_iter(),
                )
            })
            .or_else(|| {
                lookup_in(
                    &self.template_params,
                    normalize_path(&path)
                        .split('.')
                        .collect::<Vec<_>>()
                        .into_iter(),
                )
            }),
        };

        match (found, default) {
            (Some(value), _) if !value.is_null() => Some(value),
            (None, Some(fallback)) | (Some(Value::Null), Some(fallback)) => Some(fallback),
            (other, _) => other,
        }
    }
}

fn lookup_in<'a, I>(value: &Value, mut parts: I) -> Option<Value>
where
    I: Iterator<Item = &'a str>,
{
    let mut current = value;
    for part in parts.by_ref() {
        match current {
            Value::Object(map) => current = map.get(part)?,
            Value::Array(items) => {
                let idx: usize = part.parse().ok()?;
                current = items.get(idx)?;
            }
            _ => return None,
        }
    }
    Some(current.clone())
}

/// Attempt to load external i18n translations from pack assets.
///
/// When `card_spec.i18n_bundle_path` is set the function tries two resolution
/// strategies via the host asset resolver:
///   1. `{bundle_path}/{locale}.json` — locale-specific file
///   2. `{bundle_path}/en.json`       — English fallback
///   3. `{bundle_path}`               — treat the path itself as a JSON file
///
/// Successfully loaded JSON is merged into the external i18n bundle so that
/// `{{i18n:KEY}}` tokens resolve against it.
fn load_external_i18n_bundle(inv: &AdaptiveCardInvocation, locale: &str) {
    let Some(bundle_path) = inv.card_spec.i18n_bundle_path.as_deref() else {
        return;
    };
    let bundle_path = bundle_path.trim().trim_end_matches('/');
    if bundle_path.is_empty() {
        return;
    }

    // Clear any previously loaded external entries so successive invocations
    // with different packs don't bleed translations.
    i18n::clear_external_bundle();

    // Build candidate paths for the requested locale and an English fallback.
    let locale_file = format!("{bundle_path}/{locale}.json");
    let en_file = format!("{bundle_path}/en.json");

    // Try locale-specific file first.
    if let Some(json_str) = try_resolve_asset(&locale_file) {
        let _ = i18n::load_external_locale(locale, &json_str);
    }
    // Always try English as a fallback layer (unless locale is already "en").
    if !locale.eq_ignore_ascii_case("en")
        && let Some(json_str) = try_resolve_asset(&en_file)
    {
        let _ = i18n::load_external_locale("en", &json_str);
    }
    // If the path looks like a direct JSON file (ends with `.json`), also try
    // loading it as-is under the requested locale.
    if bundle_path.ends_with(".json")
        && let Some(json_str) = try_resolve_asset(bundle_path)
    {
        let _ = i18n::load_external_locale(locale, &json_str);
    }
}

/// Try to read a text asset through the host resolver, returning `None` on
/// any error or missing file.
fn try_resolve_asset(name: &str) -> Option<String> {
    resolve_with_host(name).ok().flatten()
}

fn apply_i18n_markers(value: &mut Value, locale: &str) {
    match value {
        Value::String(text) => {
            *text = replace_i18n_tokens(text, locale);
        }
        Value::Array(items) => {
            for item in items {
                apply_i18n_markers(item, locale);
            }
        }
        Value::Object(map) => {
            for entry in map.values_mut() {
                apply_i18n_markers(entry, locale);
            }
        }
        _ => {}
    }
}

fn replace_i18n_tokens(text: &str, locale: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(start) = rest.find("{{i18n:") else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..start]);
        let token_start = start + "{{i18n:".len();
        let after_start = &rest[token_start..];
        let Some(end) = after_start.find("}}") else {
            output.push_str(&rest[start..]);
            break;
        };
        let key = after_start[..end].trim();
        output.push_str(&i18n::t(locale, key));
        rest = &after_start[end + 2..];
    }
    output
}

fn apply_bindings(
    value: &mut Value,
    ctx: &BindingContext,
    engine: &dyn ExpressionEngine,
    summary: &mut BindingSummary,
) -> Result<(), ComponentError> {
    match value {
        Value::String(text) => {
            if let Some(expr) = extract_expression(text) {
                if is_simple_expression(expr) {
                    if let Some(resolved) = ctx.lookup(expr) {
                        *value = resolved;
                        summary.placeholder_replacements += 1;
                        return Ok(());
                    }
                    summary.missing_paths += 1;
                    return Err(ComponentError::Binding(format!(
                        "missing binding path: {expr}"
                    )));
                }
                if let Some(resolved) = engine.eval(expr, ctx) {
                    *value = match resolved {
                        Value::String(_) => resolved,
                        other => Value::String(stringify_value(&other)),
                    };
                    summary.expression_evaluations += 1;
                    return Ok(());
                }
                summary.missing_paths += 1;
                return Err(ComponentError::Binding(format!(
                    "invalid expression: {expr}"
                )));
            }
            if let Some(path) = extract_single_placeholder(text) {
                if let Some(resolved) = ctx.lookup(path) {
                    *value = resolved;
                    summary.placeholder_replacements += 1;
                    return Ok(());
                }
                summary.missing_paths += 1;
                return Err(ComponentError::Binding(format!(
                    "missing binding path: {path}"
                )));
            }
            let replaced = replace_placeholders(text, ctx, summary)?;
            *value = Value::String(replaced);
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                apply_bindings(item, ctx, engine, summary)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for entry in map.values_mut() {
                apply_bindings(entry, ctx, engine, summary)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn apply_handlebars(
    value: &mut Value,
    inv: &AdaptiveCardInvocation,
    summary: &mut BindingSummary,
) -> Result<(), ComponentError> {
    let mut engine = Handlebars::new();
    engine.set_strict_mode(false);
    let context = build_handlebars_context(inv);
    render_handlebars_value(value, &engine, &context, summary)
}

fn render_handlebars_value(
    value: &mut Value,
    engine: &Handlebars<'_>,
    context: &Value,
    summary: &mut BindingSummary,
) -> Result<(), ComponentError> {
    match value {
        Value::String(text) => {
            let rendered = engine
                .render_template(text, context)
                .map_err(|err| ComponentError::Binding(format!("handlebars: {err}")))?;
            *value = Value::String(rendered);
            summary.handlebars_expansions += 1;
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                render_handlebars_value(item, engine, context, summary)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for entry in map.values_mut() {
                render_handlebars_value(entry, engine, context, summary)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn build_handlebars_context(inv: &AdaptiveCardInvocation) -> Value {
    let mut root = Map::new();
    root.insert("payload".to_owned(), inv.payload.clone());
    root.insert("state".to_owned(), inv.state.clone());

    if let Some(node_id) = inv.node_id.as_deref() {
        root.insert("node_id".to_owned(), Value::String(node_id.to_owned()));
        if let Some(node) = resolve_state_node(&inv.state, node_id) {
            if let Some(payload) = node.get("payload") {
                root.insert("node_payload".to_owned(), payload.clone());
            }
            root.insert("node".to_owned(), Value::Object(node));
        }
    }

    if let Some(state_input) = resolve_state_input(&inv.state) {
        for (key, value) in state_input {
            if is_reserved_handlebars_key(&key) || root.contains_key(&key) {
                continue;
            }
            root.insert(key, value);
        }
    }

    Value::Object(root)
}

fn resolve_state_node(state: &Value, node_id: &str) -> Option<Map<String, Value>> {
    let nodes = state.get("nodes")?.as_object()?;
    let node = nodes.get(node_id)?.as_object()?;
    Some(node.clone())
}

fn resolve_state_input(state: &Value) -> Option<Map<String, Value>> {
    state.get("input")?.as_object().cloned()
}

fn is_reserved_handlebars_key(key: &str) -> bool {
    matches!(
        key,
        "payload" | "state" | "node" | "node_id" | "node_payload"
    )
}

fn replace_placeholders(
    input: &str,
    ctx: &BindingContext,
    summary: &mut BindingSummary,
) -> Result<String, ComponentError> {
    let mut output = String::new();
    let mut cursor = 0;
    let bytes = input.as_bytes();
    while cursor < input.len() {
        let remaining = &input[cursor..];
        let next_at = remaining.find("@{");
        let next_dollar = remaining.find("${");
        let next_pos = match (next_at, next_dollar) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };

        let Some(pos) = next_pos else {
            output.push_str(&input[cursor..]);
            break;
        };

        let absolute = cursor + pos;
        output.push_str(&input[cursor..absolute]);

        let marker = input.as_bytes()[absolute];
        if absolute + 2 > input.len() || bytes[absolute + 1] != b'{' {
            output.push_str(&input[absolute..]);
            break;
        }
        let rest = &input[absolute + 2..];
        if let Some(end) = rest.find('}') {
            let path = &rest[..end];
            let Some(replacement) = ctx.lookup(path.trim()) else {
                summary.missing_paths += 1;
                return Err(ComponentError::Binding(format!(
                    "missing binding path: {path}"
                )));
            };
            let replacement = match replacement {
                Value::String(s) => s,
                other => other.to_string(),
            };
            output.push_str(&replacement);
            summary.placeholder_replacements += 1;
            cursor = absolute + 2 + end + 1;
        } else {
            output.push(marker as char);
            cursor = absolute + 1;
        }
    }

    Ok(output)
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn hash_json(value: &Value) -> Option<String> {
    let bytes = serde_json::to_vec(value).ok()?;
    Some(hash_bytes(&bytes))
}

fn extract_single_placeholder(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if let Some(stripped) = trimmed.strip_prefix("@{").and_then(|s| s.strip_suffix('}')) {
        return Some(stripped.trim());
    }
    None
}

fn parse_binding_path(raw: &str) -> (String, Option<Value>) {
    let mut parts = raw.splitn(2, "||");
    let path = parts.next().unwrap_or(raw).trim().to_string();
    let default = parts.next().and_then(|d| {
        let trimmed = d.trim();
        if trimmed.is_empty() {
            return None;
        }
        serde_json::from_str::<Value>(trimmed)
            .ok()
            .or_else(|| Some(Value::String(trimmed.to_string())))
    });
    (path, default)
}

fn extract_expression(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if let Some(stripped) = trimmed.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        return Some(stripped.trim());
    }
    None
}

fn is_simple_expression(expr: &str) -> bool {
    let trimmed = expr.trim();
    if trimmed.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    !trimmed.contains('?') && !trimmed.contains("==") && !trimmed.contains(':')
}

fn normalize_path(path: &str) -> String {
    let mut normalized = path.replace('[', ".").replace(']', "");
    normalized = normalized.replace("..", ".");
    normalized.trim_matches('.').to_string()
}

pub fn analyze_features(card: &Value) -> CardFeatureSummary {
    let mut used_elements = BTreeSet::new();
    let mut used_actions = BTreeSet::new();
    let mut summary = CardFeatureSummary {
        version: card
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        ..CardFeatureSummary::default()
    };

    fn merge_requires(target: &mut Value, new_value: &Value) {
        match target {
            Value::Object(dst) => {
                if let Value::Object(src) = new_value {
                    for (k, v) in src {
                        dst.entry(k.clone()).or_insert(v.clone());
                    }
                }
            }
            Value::Null => *target = new_value.clone(),
            _ => {}
        }
    }

    fn walk(
        value: &Value,
        used_elements: &mut BTreeSet<String>,
        used_actions: &mut BTreeSet<String>,
        summary: &mut CardFeatureSummary,
    ) {
        match value {
            Value::Object(map) => {
                if let Some(kind) = map.get("type").and_then(|v| v.as_str()) {
                    if kind.starts_with("Action.") {
                        used_actions.insert(kind.to_string());
                        if kind == "Action.ShowCard" {
                            summary.uses_show_card = true;
                        }
                        if kind == "Action.ToggleVisibility" {
                            summary.uses_toggle_visibility = true;
                        }
                    } else {
                        used_elements.insert(kind.to_string());
                        if kind == "Media" {
                            summary.uses_media = true;
                        }
                    }
                }
                if map.contains_key("authentication") {
                    summary.uses_auth = true;
                }
                if let Some(req) = map.get("requires") {
                    merge_requires(&mut summary.requires_features, req);
                }
                for value in map.values() {
                    walk(value, used_elements, used_actions, summary);
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, used_elements, used_actions, summary);
                }
            }
            _ => {}
        }
    }

    walk(card, &mut used_elements, &mut used_actions, &mut summary);
    summary.used_elements = used_elements.into_iter().collect();
    summary.used_actions = used_actions.into_iter().collect();
    summary
}

pub fn validate_card(card: &Value, locale: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if !card.is_object() {
        issues.push(ValidationIssue {
            code: "invalid-root".into(),
            msg_key: Some("validation.card.invalid_root".into()),
            message: i18n::t(locale, "validation.card.invalid_root"),
            path: "/".into(),
        });
        return issues;
    }

    let type_value = card.get("type").and_then(|v| v.as_str());
    if type_value != Some("AdaptiveCard") {
        issues.push(ValidationIssue {
            code: "invalid-type".into(),
            msg_key: Some("validation.card.invalid_type".into()),
            message: i18n::t(locale, "validation.card.invalid_type"),
            path: "/type".into(),
        });
    }
    if card.get("version").is_none() {
        issues.push(ValidationIssue {
            code: "missing-version".into(),
            msg_key: Some("validation.card.missing_version".into()),
            message: i18n::t(locale, "validation.card.missing_version"),
            path: "/version".into(),
        });
    }

    let mut input_ids = HashSet::new();

    fn push_issue(
        locale: &str,
        path: &str,
        code: &str,
        fallback_message: &str,
        issues: &mut Vec<ValidationIssue>,
    ) {
        let msg_key = format!("validation.card.{code}");
        let message = i18n::t(locale, &msg_key);
        let message = if message == msg_key {
            fallback_message.to_string()
        } else {
            message
        };
        issues.push(ValidationIssue {
            code: code.to_string(),
            msg_key: Some(msg_key),
            message,
            path: path.to_string(),
        });
    }

    fn visit(
        value: &Value,
        path: &str,
        locale: &str,
        issues: &mut Vec<ValidationIssue>,
        input_ids: &mut HashSet<String>,
        action_ids: &mut HashSet<String>,
    ) {
        match value {
            Value::Object(map) => {
                let kind = map.get("type").and_then(|v| v.as_str()).unwrap_or_default();
                if kind.starts_with("Input.") && !map.contains_key("id") {
                    push_issue(
                        locale,
                        path,
                        "missing-id",
                        "Inputs must include an id",
                        issues,
                    );
                }
                if kind.starts_with("Input.")
                    && let Some(id) = map.get("id").and_then(|v| v.as_str())
                {
                    let inserted = input_ids.insert(id.to_string());
                    if !inserted {
                        push_issue(
                            locale,
                            path,
                            "duplicate-id",
                            "Input ids should be unique within the card",
                            issues,
                        );
                    }
                }
                if kind.starts_with("Action.") {
                    if let Some(id) = map.get("id").and_then(|v| v.as_str())
                        && !action_ids.insert(id.to_string())
                    {
                        push_issue(
                            locale,
                            path,
                            "duplicate-action-id",
                            "Action ids should be unique within the card",
                            issues,
                        );
                    }
                    validate_action(map, path, locale, issues);
                }
                match kind {
                    "Input.ChoiceSet" => {
                        if let Some(choices) = map.get("choices") {
                            if let Some(arr) = choices.as_array() {
                                if arr.is_empty() {
                                    push_issue(
                                        locale,
                                        path,
                                        "empty-choices",
                                        "Input.ChoiceSet must include at least one choice",
                                        issues,
                                    );
                                } else if arr.iter().any(|c| {
                                    !c.get("title")
                                        .and_then(|v| v.as_str())
                                        .map(|s| !s.is_empty())
                                        .unwrap_or(false)
                                        || !c
                                            .get("value")
                                            .and_then(|v| v.as_str())
                                            .map(|s| !s.is_empty())
                                            .unwrap_or(false)
                                }) {
                                    push_issue(
                                        locale,
                                        path,
                                        "invalid-choice",
                                        "Choices must include non-empty title and value",
                                        issues,
                                    );
                                }
                            } else {
                                push_issue(
                                    locale,
                                    path,
                                    "invalid-choices",
                                    "Input.ChoiceSet choices must be an array",
                                    issues,
                                );
                            }
                        } else {
                            push_issue(
                                locale,
                                path,
                                "missing-choices",
                                "Input.ChoiceSet must include choices",
                                issues,
                            );
                        }
                    }
                    "Input.Toggle" => {
                        if map
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .is_empty()
                        {
                            push_issue(
                                locale,
                                path,
                                "missing-title",
                                "Input.Toggle should include a title",
                                issues,
                            );
                        }
                    }
                    "Input.Number" => {
                        if let (Some(min), Some(max)) = (
                            map.get("min").and_then(|v| v.as_f64()),
                            map.get("max").and_then(|v| v.as_f64()),
                        ) && min > max
                        {
                            push_issue(
                                locale,
                                path,
                                "invalid-range",
                                "Input.Number min must be <= max",
                                issues,
                            );
                        }
                    }
                    "ColumnSet" => {
                        if let Some(columns) = map.get("columns") {
                            if !columns.is_array() {
                                push_issue(
                                    locale,
                                    path,
                                    "invalid-columns",
                                    "ColumnSet columns must be an array",
                                    issues,
                                );
                            } else if columns.as_array().map(|c| c.is_empty()).unwrap_or(false) {
                                push_issue(
                                    locale,
                                    path,
                                    "empty-columns",
                                    "ColumnSet columns must not be empty",
                                    issues,
                                );
                            }
                        }
                    }
                    "Media" => {
                        if let Some(sources) = map.get("sources") {
                            if !sources.is_array() {
                                push_issue(
                                    locale,
                                    path,
                                    "invalid-sources",
                                    "Media sources must be an array",
                                    issues,
                                );
                            } else if sources.as_array().map(|s| s.is_empty()).unwrap_or(false) {
                                push_issue(
                                    locale,
                                    path,
                                    "missing-sources",
                                    "Media must include at least one source",
                                    issues,
                                );
                            } else if sources
                                .as_array()
                                .map(|arr| {
                                    arr.iter().any(|s| {
                                        !s.get("url")
                                            .and_then(|v| v.as_str())
                                            .map(|v| !v.is_empty())
                                            .unwrap_or(false)
                                    })
                                })
                                .unwrap_or(false)
                            {
                                push_issue(
                                    locale,
                                    path,
                                    "invalid-source",
                                    "Media sources must include non-empty url",
                                    issues,
                                );
                            }
                        } else {
                            push_issue(
                                locale,
                                path,
                                "missing-sources",
                                "Media must include sources",
                                issues,
                            );
                        }
                    }
                    _ => {}
                }
                for (key, value) in map {
                    let child_path = format!("{}/{}", path, key);
                    visit(value, &child_path, locale, issues, input_ids, action_ids);
                }
            }
            Value::Array(items) => {
                for (idx, item) in items.iter().enumerate() {
                    let child_path = format!("{}/{}", path, idx);
                    visit(item, &child_path, locale, issues, input_ids, action_ids);
                }
            }
            _ => {}
        }
    }

    fn validate_action(
        map: &Map<String, Value>,
        path: &str,
        locale: &str,
        issues: &mut Vec<ValidationIssue>,
    ) {
        let kind = map.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        match kind {
            "Action.OpenUrl" => {
                if !map
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                {
                    push_issue(
                        locale,
                        path,
                        "missing-url",
                        "Action.OpenUrl must include a url",
                        issues,
                    );
                }
            }
            "Action.Execute" => {
                if map.get("verb").and_then(|v| v.as_str()).is_none() {
                    push_issue(
                        locale,
                        path,
                        "missing-verb",
                        "Action.Execute should include a verb",
                        issues,
                    );
                }
                if map
                    .get("data")
                    .map(|d| !d.is_object() && !d.is_null())
                    .unwrap_or(false)
                {
                    push_issue(
                        locale,
                        path,
                        "invalid-data",
                        "Action.Execute data should be an object when present",
                        issues,
                    );
                }
            }
            "Action.ShowCard" => {
                if !map.contains_key("card") {
                    push_issue(
                        locale,
                        path,
                        "missing-card",
                        "Action.ShowCard must include a card",
                        issues,
                    );
                }
                if let Some(card_value) = map.get("card")
                    && !card_value.is_object()
                {
                    push_issue(
                        locale,
                        path,
                        "invalid-card",
                        "Action.ShowCard card must be an object",
                        issues,
                    );
                }
            }
            "Action.ToggleVisibility" => {
                if !map.contains_key("targetElements") {
                    push_issue(
                        locale,
                        path,
                        "missing-target-elements",
                        "Action.ToggleVisibility must include targetElements",
                        issues,
                    );
                } else if map
                    .get("targetElements")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.is_empty())
                    .unwrap_or(false)
                {
                    push_issue(
                        locale,
                        path,
                        "empty-target-elements",
                        "Action.ToggleVisibility targetElements must not be empty",
                        issues,
                    );
                }
            }
            _ => {}
        }
    }

    if let Some(body) = card.get("body")
        && !body.is_array()
    {
        push_issue(
            locale,
            "/body",
            "invalid-body",
            "body must be an array",
            &mut issues,
        );
    }
    if let Some(actions) = card.get("actions")
        && !actions.is_array()
    {
        push_issue(
            locale,
            "/actions",
            "invalid-actions",
            "actions must be an array",
            &mut issues,
        );
    }

    let mut action_ids = HashSet::new();
    visit(
        card,
        "",
        locale,
        &mut issues,
        &mut input_ids,
        &mut action_ids,
    );
    issues
}
