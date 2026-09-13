//! Model pricing policy enforcement.
//!
//! This is the backend side of the *free-only* guarantee: even if a UI only
//! *displays* free models, every request is re-validated here before any
//! HTTP call leaves the machine. There is intentionally no API to approve a
//! paid model — the only decision this module can express is
//! [`ModelPricingPolicy::FreeOnly`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Pricing policy applied to model selection. OpenRouter is always
/// [`ModelPricingPolicy::FreeOnly`]; the enum exists so other providers can
/// adopt explicit policies later without changing call sites.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPricingPolicy {
    #[default]
    FreeOnly,
}

/// Rejection of a model that violates the pricing policy.
#[derive(Debug, Error)]
pub enum PolicyViolation {
    #[error("model '{model}' is not free (prompt={prompt}, completion={completion}): paid models are forbidden")]
    ModelNotFree {
        model: String,
        prompt: f64,
        completion: f64,
    },
    #[error("model '{0}' is unknown: only verified free models may be used")]
    UnknownModel(String),
}

/// Pricing gate: both prices must be exactly zero.
///
/// Unknown or unparsable prices are rejected (fail closed).
pub fn check_free_only(
    model: &str,
    prompt_price: f64,
    completion_price: f64,
) -> Result<(), PolicyViolation> {
    if prompt_price == 0.0 && completion_price == 0.0 {
        Ok(())
    } else {
        Err(PolicyViolation::ModelNotFree {
            model: model.to_string(),
            prompt: prompt_price,
            completion: completion_price,
        })
    }
}

/// Parse an OpenRouter-style price (`"0"`, `"0.000001"`, `0`, …).
/// Anything unparsable yields `None` so callers fail closed.
pub fn parse_price(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_zero_is_free() {
        assert!(check_free_only("m", 0.0, 0.0).is_ok());
    }

    #[test]
    fn any_positive_price_is_rejected() {
        assert!(check_free_only("m", 0.000001, 0.0).is_err());
        assert!(check_free_only("m", 0.0, 0.000001).is_err());
        assert!(check_free_only("m", 1.0, 1.0).is_err());
    }

    #[test]
    fn prices_parse_from_strings_and_numbers() {
        assert_eq!(parse_price(&serde_json::json!("0")), Some(0.0));
        assert_eq!(parse_price(&serde_json::json!("0.000002")), Some(0.000002));
        assert_eq!(parse_price(&serde_json::json!(0)), Some(0.0));
        assert_eq!(parse_price(&serde_json::Value::Null), None);
        assert_eq!(parse_price(&serde_json::json!("n/a")), None);
    }
}
