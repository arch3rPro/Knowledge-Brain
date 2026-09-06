use kb_core::KbError;
use kb_protocol::ErrorEnvelope;

#[test]
fn error_envelope_has_stable_machine_fields() {
    let value = serde_json::to_value(ErrorEnvelope::from(KbError::invalid_config(
        "admission.yml",
        "directories must be a sequence",
    )))
    .unwrap();

    assert_eq!(value["schema_version"], "v1.0");
    assert_eq!(value["error"]["code"], "invalid_config");
    assert_eq!(value["error"]["retryable"], false);
    assert_eq!(
        value["error"]["next_action"],
        "Correct admission.yml and run the command again."
    );
    assert_eq!(value["error"]["details"]["path"], "admission.yml");
}
