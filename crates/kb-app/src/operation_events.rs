use std::{fs, path::Path};

use kb_core::{
    CURRENT_SCHEMA_VERSION, KbError, OperationEvent, OperationEventKind, OperationEventLog,
    OperationEventReport, OperationId,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{OperationState, UserPaths, inspect_operation, operation::operation_directory};

pub(crate) const MAX_OPERATION_EVENTS: usize = 10_100;

pub(crate) fn record_operation_event(
    user_paths: &UserPaths,
    operation_id: OperationId,
    kind: OperationEventKind,
    progress: Option<(u64, u64)>,
    message: &str,
    recorded_at: OffsetDateTime,
) -> Result<OperationEvent, KbError> {
    let path = event_path(user_paths, operation_id);
    let mut log = if path.is_file() {
        let log: OperationEventLog = crate::operation::read_json(&path)?;
        log.validate(operation_id, MAX_OPERATION_EVENTS)?;
        log
    } else {
        OperationEventLog {
            schema_version: CURRENT_SCHEMA_VERSION,
            operation_id,
            events: Vec::new(),
        }
    };
    if let Some(last) = log.events.last()
        && last.kind == kind
        && (last.completed, last.total) == split_progress(progress)
    {
        return Ok(last.clone());
    }
    if log.events.len() >= MAX_OPERATION_EVENTS {
        return Err(KbError::invalid_config(
            path.display().to_string(),
            "operation event limit exceeded",
        ));
    }
    let (completed, total) = split_progress(progress);
    let event = OperationEvent {
        id: log.events.len() as u64 + 1,
        kind,
        completed,
        total,
        message: message.to_owned(),
        recorded_at: format_time(recorded_at)?,
    };
    log.events.push(event.clone());
    log.validate(operation_id, MAX_OPERATION_EVENTS)?;
    crate::operation::create_private_directory_all(
        path.parent()
            .expect("operation event path always has a parent"),
    )?;
    crate::operation::write_json(&path, &log)?;
    Ok(event)
}

pub fn operation_events(
    user_paths: &UserPaths,
    operation_id: OperationId,
) -> Result<OperationEventReport, KbError> {
    let path = event_path(user_paths, operation_id);
    let log = if path.is_file() {
        let log: OperationEventLog = crate::operation::read_json(&path)?;
        log.validate(operation_id, MAX_OPERATION_EVENTS)?;
        log
    } else {
        synthetic_log(user_paths, operation_id)?
    };
    Ok(OperationEventReport {
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id,
        terminal: log.is_terminal(),
        events: log.events,
    })
}

fn synthetic_log(
    user_paths: &UserPaths,
    operation_id: OperationId,
) -> Result<OperationEventLog, KbError> {
    let state = inspect_operation(user_paths, operation_id)?;
    let (kind, progress, message, recorded_at) = match state {
        OperationState::Planned(value) => (
            OperationEventKind::Planned,
            Some((0, value.creates.len() as u64)),
            "Operation plan is ready for review.",
            value.created_at,
        ),
        OperationState::PlannedSource(value) => (
            OperationEventKind::Planned,
            Some((0, value.writes.len() as u64)),
            "Source capture plan is ready for review.",
            value.created_at,
        ),
        OperationState::PlannedKnowledge(value) => (
            OperationEventKind::Planned,
            Some((0, value.writes.len() as u64)),
            "Knowledge save plan is ready for review.",
            value.created_at,
        ),
        OperationState::Applied(value) => (
            OperationEventKind::Applied,
            Some((value.created.len() as u64, value.created.len() as u64)),
            "Operation is complete.",
            modified_at(&operation_directory(user_paths, operation_id).join("result.json"))?,
        ),
        OperationState::AppliedSource(value) => {
            let count = (value.captured.len() + value.marked_missing.len()) as u64;
            (
                OperationEventKind::Applied,
                Some((count, count)),
                "Source capture is complete.",
                modified_at(&operation_directory(user_paths, operation_id).join("result.json"))?,
            )
        }
        OperationState::AppliedKnowledge(value) => {
            let count = value.changed.len() as u64;
            (
                OperationEventKind::Applied,
                Some((count, count)),
                "Knowledge save is complete.",
                modified_at(&operation_directory(user_paths, operation_id).join("result.json"))?,
            )
        }
    };
    let (completed, total) = split_progress(progress);
    Ok(OperationEventLog {
        schema_version: CURRENT_SCHEMA_VERSION,
        operation_id,
        events: vec![OperationEvent {
            id: 1,
            kind,
            completed,
            total,
            message: message.into(),
            recorded_at,
        }],
    })
}

fn event_path(user_paths: &UserPaths, operation_id: OperationId) -> std::path::PathBuf {
    operation_directory(user_paths, operation_id).join("events.json")
}

fn split_progress(progress: Option<(u64, u64)>) -> (Option<u64>, Option<u64>) {
    progress.map_or((None, None), |(completed, total)| {
        (Some(completed), Some(total))
    })
}

fn modified_at(path: &Path) -> Result<String, KbError> {
    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| {
            KbError::io_failure(
                "read operation result timestamp",
                path.display().to_string(),
                error.to_string(),
            )
        })?;
    format_time(OffsetDateTime::from(modified))
}

fn format_time(value: OffsetDateTime) -> Result<String, KbError> {
    value
        .format(&Rfc3339)
        .map_err(|error| KbError::invalid_config("operation event timestamp", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_is_ordered_and_ignores_an_identical_last_transition() {
        let temporary = tempfile::tempdir().unwrap();
        let paths = UserPaths::new(
            temporary.path().join("config"),
            temporary.path().join("state"),
            temporary.path().join("cache"),
        );
        let operation_id = OperationId::new();
        let at = OffsetDateTime::from_unix_timestamp(1_788_753_600).unwrap();

        let first = record_operation_event(
            &paths,
            operation_id,
            OperationEventKind::Planned,
            Some((0, 2)),
            "Plan is ready.",
            at,
        )
        .unwrap();
        let duplicate = record_operation_event(
            &paths,
            operation_id,
            OperationEventKind::Planned,
            Some((0, 2)),
            "Different wording is not a new transition.",
            at,
        )
        .unwrap();
        let second = record_operation_event(
            &paths,
            operation_id,
            OperationEventKind::Applying,
            Some((0, 2)),
            "Apply started.",
            at,
        )
        .unwrap();

        assert_eq!(first, duplicate);
        assert_eq!(second.id, 2);
        assert_eq!(
            operation_events(&paths, operation_id).unwrap().events.len(),
            2
        );
    }
}
