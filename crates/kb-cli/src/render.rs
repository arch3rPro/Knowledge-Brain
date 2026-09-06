use std::process::ExitCode;

use kb_core::KbError;
use kb_protocol::{Envelope, ErrorEnvelope};
use serde_json::Value;

pub(crate) fn success(value: &Value, json_output: bool) -> ExitCode {
    let rendered = if json_output {
        serde_json::to_string(&Envelope::new(value))
    } else if let Some(diff) = value.get("diff").and_then(Value::as_str) {
        Ok(diff.to_owned())
    } else {
        serde_json::to_string_pretty(value)
    };
    match rendered {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(serialization_error) => error(
            KbError::invalid_config("command response", serialization_error.to_string()),
            json_output,
        ),
    }
}

pub(crate) fn error(error: KbError, json_output: bool) -> ExitCode {
    if json_output {
        match serde_json::to_string(&ErrorEnvelope::from(error)) {
            Ok(text) => println!("{text}"),
            Err(serialization_error) => eprintln!("Error: {serialization_error}"),
        }
    } else {
        eprintln!("Error: {error}");
    }
    ExitCode::FAILURE
}
