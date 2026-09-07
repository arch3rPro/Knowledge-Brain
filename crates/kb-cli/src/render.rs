use std::{io::Write, process::ExitCode};

use kb_core::KbError;
use kb_protocol::{Envelope, ErrorEnvelope};
use serde_json::Value;

pub(crate) fn success(value: &Value, json_output: bool, fail_on_findings: bool) -> ExitCode {
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
            if fail_on_findings
                && value
                    .get("findings")
                    .and_then(Value::as_array)
                    .is_some_and(|findings| !findings.is_empty())
            {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
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

pub(crate) fn startup(value: &Value) -> Result<(), KbError> {
    let text = serde_json::to_string(&Envelope::new(value))
        .map_err(|error| KbError::invalid_config("server startup response", error.to_string()))?;
    println!("{text}");
    std::io::stdout().flush().map_err(|error| {
        KbError::io_failure("flush server startup response", "stdout", error.to_string())
    })
}
