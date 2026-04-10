use std::panic::{AssertUnwindSafe, catch_unwind};

use once_cell::sync::Lazy;
use serde_json::Value;

use jsonschema::error::ValidationErrorKind;
use jsonschema::{Validator, draft7};

use crate::model::ValidationIssue;

static INVOCATION_SCHEMA: Lazy<Validator> = Lazy::new(|| {
    let schema: Value = serde_json::from_str(include_str!(
        "../schemas/adaptive-card.invocation.v1.schema.json"
    ))
    .expect("invocation schema JSON must be valid");
    draft7::options()
        .build(&schema)
        .expect("invocation schema must compile")
});

pub fn locate_invocation_candidate(value: &Value) -> Option<Value> {
    if let Some(inv) = find_invocation_value(value) {
        return Some(inv);
    }
    if let Some(payload) = value.get("payload")
        && payload.is_object()
    {
        return Some(payload.clone());
    }
    if let Some(config) = value.get("config")
        && config.is_object()
    {
        return Some(config.clone());
    }
    None
}

pub fn validate_invocation_schema(value: &Value) -> Vec<ValidationIssue> {
    collect_validation_issues(|| {
        INVOCATION_SCHEMA
            .iter_errors(value)
            .map(|error| map_schema_error(&error))
            .collect()
    })
}

fn collect_validation_issues<F>(collect: F) -> Vec<ValidationIssue>
where
    F: FnOnce() -> Vec<ValidationIssue>,
{
    catch_unwind(AssertUnwindSafe(collect))
        .unwrap_or_else(|_| vec![schema_validator_panic_issue()])
}

fn schema_validator_panic_issue() -> ValidationIssue {
    ValidationIssue {
        code: "AC_INVOCATION_SCHEMA_ERROR".to_string(),
        msg_key: Some("validation.invocation.schema_error".to_string()),
        message: "Invocation schema validator panicked".to_string(),
        path: "/".to_string(),
    }
}

fn map_schema_error(error: &jsonschema::ValidationError) -> ValidationIssue {
    let (code, msg_key) = match error.kind() {
        ValidationErrorKind::Required { .. } => (
            "AC_INVOCATION_MISSING_FIELD",
            "validation.invocation.missing_field",
        ),
        ValidationErrorKind::Type { .. } => (
            "AC_INVOCATION_INVALID_TYPE",
            "validation.invocation.invalid_type",
        ),
        ValidationErrorKind::Enum { .. } => (
            "AC_INVOCATION_INVALID_ENUM",
            "validation.invocation.invalid_enum",
        ),
        _ => (
            "AC_INVOCATION_SCHEMA_ERROR",
            "validation.invocation.schema_error",
        ),
    };
    let raw_path = error.instance_path().to_string();
    let path = if raw_path.is_empty() {
        "/".to_string()
    } else {
        raw_path
    };
    ValidationIssue {
        code: code.to_string(),
        msg_key: Some(msg_key.to_string()),
        message: error.to_string(),
        path,
    }
}

fn find_invocation_value(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    if obj.contains_key("card_source") || obj.contains_key("card_spec") {
        return Some(value.clone());
    }
    if let Some(inv) = obj.get("invocation") {
        return Some(inv.clone());
    }
    if let Some(card) = obj.get("card") {
        return Some(card.clone());
    }
    if let Some(payload) = obj.get("payload")
        && payload
            .as_object()
            .map(|p| p.contains_key("card_source") || p.contains_key("card_spec"))
            .unwrap_or(false)
    {
        return Some(payload.clone());
    }
    if let Some(config) = obj.get("config") {
        if config
            .as_object()
            .map(|c| c.contains_key("card_source") || c.contains_key("card_spec"))
            .unwrap_or(false)
        {
            return Some(config.clone());
        }
        if let Some(card) = config.get("card") {
            return Some(card.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{collect_validation_issues, validate_invocation_schema};

    #[test]
    fn schema_validation_returns_issues_for_invalid_input() {
        let issues = validate_invocation_schema(&serde_json::json!({}));

        assert!(!issues.is_empty());
        assert_eq!(issues[0].code, "AC_INVOCATION_MISSING_FIELD");
    }

    #[test]
    fn schema_validation_converts_panics_into_schema_errors() {
        let issues = collect_validation_issues(|| panic!("boom"));

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "AC_INVOCATION_SCHEMA_ERROR");
        assert_eq!(issues[0].path, "/");
    }
}
