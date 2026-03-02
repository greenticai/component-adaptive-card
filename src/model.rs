use greentic_interfaces_guest::component_v0_6::node;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type CanonicalInvocationEnvelope = node::InvocationEnvelope;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CardSource {
    #[default]
    Inline,
    Asset,
    Catalog,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CardSpec {
    pub inline_json: Option<Value>,
    pub asset_path: Option<String>,
    pub catalog_name: Option<String>,
    pub template_params: Option<Value>,
    pub asset_registry: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum InvocationMode {
    Render,
    Validate,
    #[default]
    RenderAndValidate,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ValidationMode {
    Off,
    #[default]
    Warn,
    Error,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdaptiveCardInvocation {
    #[serde(default)]
    #[serde(alias = "card_source")]
    pub card_source: CardSource,
    #[serde(default)]
    #[serde(alias = "card_spec")]
    pub card_spec: CardSpec,

    #[serde(default)]
    #[serde(alias = "node_id")]
    pub node_id: Option<String>,
    #[serde(default)]
    #[serde(alias = "i18n_locale")]
    pub locale: Option<String>,

    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub session: Value,
    #[serde(default)]
    pub state: Value,

    #[serde(default)]
    pub interaction: Option<CardInteraction>,

    #[serde(default)]
    pub mode: InvocationMode,

    #[serde(default)]
    #[serde(alias = "validation_mode")]
    pub validation_mode: ValidationMode,

    /// Optional shared invocation envelope metadata from the host.
    #[serde(default)]
    #[serde(
        deserialize_with = "deserialize_canonical_invocation_envelope_lenient",
        serialize_with = "serialize_canonical_invocation_envelope_opt"
    )]
    pub envelope: Option<CanonicalInvocationEnvelope>,
}

impl PartialEq for AdaptiveCardInvocation {
    fn eq(&self, other: &Self) -> bool {
        self.card_source == other.card_source
            && self.card_spec == other.card_spec
            && self.node_id == other.node_id
            && self.locale == other.locale
            && self.payload == other.payload
            && self.session == other.session
            && self.state == other.state
            && self.interaction == other.interaction
            && self.mode == other.mode
            && self.validation_mode == other.validation_mode
            && canonical_envelope_eq(&self.envelope, &other.envelope)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum CardInteractionType {
    #[default]
    Submit,
    Execute,
    OpenUrl,
    ShowCard,
    ToggleVisibility,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardInteraction {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(alias = "interaction_type")]
    pub interaction_type: CardInteractionType,
    #[serde(alias = "action_id")]
    pub action_id: String,
    #[serde(default)]
    pub verb: Option<String>,
    #[serde(alias = "raw_inputs")]
    #[serde(default)]
    pub raw_inputs: Value,
    #[serde(alias = "card_instance_id")]
    pub card_instance_id: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum AdaptiveActionType {
    #[default]
    Submit,
    Execute,
    OpenUrl,
    ShowCard,
    ToggleVisibility,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdaptiveActionEvent {
    pub action_type: AdaptiveActionType,
    pub action_id: String,
    #[serde(default)]
    pub verb: Option<String>,
    #[serde(default)]
    pub route: Option<String>,
    #[serde(default)]
    pub inputs: Value,

    pub card_id: String,
    pub card_instance_id: String,
    #[serde(default)]
    pub subcard_id: Option<String>,

    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum StateUpdateOp {
    Set { path: String, value: Value },
    Merge { path: String, value: Value },
    Delete { path: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum SessionUpdateOp {
    SetRoute { route: String },
    SetAttribute { key: String, value: Value },
    DeleteAttribute { key: String },
    PushCardStack { card_id: String },
    PopCardStack,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardFeatureSummary {
    pub version: Option<String>,
    pub used_elements: Vec<String>,
    pub used_actions: Vec<String>,
    pub uses_show_card: bool,
    pub uses_toggle_visibility: bool,
    pub uses_media: bool,
    pub uses_auth: bool,
    #[serde(default)]
    pub requires_features: Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    pub code: String,
    #[serde(default)]
    pub msg_key: Option<String>,
    pub message: String,
    pub path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEvent {
    pub name: String,
    #[serde(default)]
    pub properties: Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdaptiveCardResult {
    pub rendered_card: Option<Value>,
    pub event: Option<AdaptiveActionEvent>,
    #[serde(default)]
    pub state_updates: Vec<StateUpdateOp>,
    #[serde(default)]
    pub session_updates: Vec<SessionUpdateOp>,
    pub card_features: CardFeatureSummary,
    #[serde(default)]
    pub validation_issues: Vec<ValidationIssue>,
    #[serde(default)]
    pub telemetry_events: Vec<TelemetryEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TenantCtxWire {
    #[serde(alias = "tenant_id")]
    tenant_id: String,
    #[serde(default, alias = "team_id")]
    team_id: Option<String>,
    #[serde(default, alias = "user_id")]
    user_id: Option<String>,
    #[serde(alias = "env_id")]
    env_id: String,
    #[serde(alias = "trace_id")]
    trace_id: String,
    #[serde(alias = "correlation_id")]
    correlation_id: String,
    #[serde(alias = "deadline_ms")]
    deadline_ms: u64,
    attempt: u32,
    #[serde(default, alias = "idempotency_key")]
    idempotency_key: Option<String>,
    #[serde(alias = "i18n_id")]
    i18n_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CanonicalInvocationEnvelopeWire {
    ctx: TenantCtxWire,
    #[serde(alias = "flow_id")]
    flow_id: String,
    #[serde(alias = "step_id")]
    step_id: String,
    #[serde(alias = "component_id")]
    component_id: String,
    attempt: u32,
    #[serde(alias = "payload_cbor")]
    payload_cbor: Vec<u8>,
    #[serde(default, alias = "metadata_cbor")]
    metadata_cbor: Option<Vec<u8>>,
}

impl From<CanonicalInvocationEnvelopeWire> for CanonicalInvocationEnvelope {
    fn from(value: CanonicalInvocationEnvelopeWire) -> Self {
        Self {
            ctx: greentic_interfaces_guest::component_v0_6::node::TenantCtx {
                tenant_id: value.ctx.tenant_id,
                team_id: value.ctx.team_id,
                user_id: value.ctx.user_id,
                env_id: value.ctx.env_id,
                trace_id: value.ctx.trace_id,
                correlation_id: value.ctx.correlation_id,
                deadline_ms: value.ctx.deadline_ms,
                attempt: value.ctx.attempt,
                idempotency_key: value.ctx.idempotency_key,
                i18n_id: value.ctx.i18n_id,
            },
            flow_id: value.flow_id,
            step_id: value.step_id,
            component_id: value.component_id,
            attempt: value.attempt,
            payload_cbor: value.payload_cbor,
            metadata_cbor: value.metadata_cbor,
        }
    }
}

impl From<&CanonicalInvocationEnvelope> for CanonicalInvocationEnvelopeWire {
    fn from(value: &CanonicalInvocationEnvelope) -> Self {
        Self {
            ctx: TenantCtxWire {
                tenant_id: value.ctx.tenant_id.clone(),
                team_id: value.ctx.team_id.clone(),
                user_id: value.ctx.user_id.clone(),
                env_id: value.ctx.env_id.clone(),
                trace_id: value.ctx.trace_id.clone(),
                correlation_id: value.ctx.correlation_id.clone(),
                deadline_ms: value.ctx.deadline_ms,
                attempt: value.ctx.attempt,
                idempotency_key: value.ctx.idempotency_key.clone(),
                i18n_id: value.ctx.i18n_id.clone(),
            },
            flow_id: value.flow_id.clone(),
            step_id: value.step_id.clone(),
            component_id: value.component_id.clone(),
            attempt: value.attempt,
            payload_cbor: value.payload_cbor.clone(),
            metadata_cbor: value.metadata_cbor.clone(),
        }
    }
}

pub fn parse_canonical_invocation_envelope(
    value: serde_json::Value,
) -> Option<CanonicalInvocationEnvelope> {
    serde_json::from_value::<CanonicalInvocationEnvelopeWire>(value)
        .ok()
        .map(Into::into)
}

fn canonical_envelope_eq(
    lhs: &Option<CanonicalInvocationEnvelope>,
    rhs: &Option<CanonicalInvocationEnvelope>,
) -> bool {
    match (lhs, rhs) {
        (Some(left), Some(right)) => {
            CanonicalInvocationEnvelopeWire::from(left)
                == CanonicalInvocationEnvelopeWire::from(right)
        }
        (None, None) => true,
        _ => false,
    }
}

pub fn deserialize_canonical_invocation_envelope_opt<'de, D>(
    deserializer: D,
) -> Result<Option<CanonicalInvocationEnvelope>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<CanonicalInvocationEnvelopeWire>::deserialize(deserializer)?;
    Ok(raw.map(Into::into))
}

/// Lenient variant: silently returns `None` when the envelope JSON is
/// incomplete (e.g. `envelope: {}` from simplified flow YAML).
fn deserialize_canonical_invocation_envelope_lenient<'de, D>(
    deserializer: D,
) -> Result<Option<CanonicalInvocationEnvelope>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<serde_json::Value> = serde::Deserialize::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(v) => Ok(serde_json::from_value::<CanonicalInvocationEnvelopeWire>(v)
            .ok()
            .map(Into::into)),
    }
}

pub fn serialize_canonical_invocation_envelope_opt<S>(
    value: &Option<CanonicalInvocationEnvelope>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(envelope) => CanonicalInvocationEnvelopeWire::from(envelope).serialize(serializer),
        None => serializer.serialize_none(),
    }
}
