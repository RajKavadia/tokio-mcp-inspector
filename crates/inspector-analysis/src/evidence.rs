//! Evidence gating: a diagnosis without observation IDs is invalid.
//! The MCP layer must reject model-generated explanations that cite no measurements.
use inspector_core::{CoreError, Diagnosis, FindingStatus};

pub fn validate_diagnosis(d: &Diagnosis) -> Result<(), CoreError> {
    if d.evidence.is_empty() {
        return Err(CoreError::Validation(
            "diagnosis cites no observation IDs; refusing to present as finding".into(),
        ));
    }
    if !(0.0..=1.0).contains(&d.confidence) {
        return Err(CoreError::Validation("confidence out of range".into()));
    }
    if d.status == FindingStatus::Verified && d.confidence < 0.95 {
        return Err(CoreError::Validation(
            "Verified requires controlled A/B evidence and confidence >= 0.95".into(),
        ));
    }
    Ok(())
}
