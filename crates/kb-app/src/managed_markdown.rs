use kb_core::KbError;

const START: &str = "<!-- kb:managed:start -->";
const END: &str = "<!-- kb:managed:end -->";

pub(crate) struct IndexEntry {
    pub path: String,
    pub title: String,
    pub description: Option<String>,
}

pub(crate) struct LogEntry {
    pub path: String,
    pub title: String,
    pub summary: String,
    pub creation: bool,
}

pub(crate) fn render_index(
    original: &str,
    research: &[IndexEntry],
    articles: &[IndexEntry],
) -> Result<String, KbError> {
    let mut body = String::new();
    render_group(&mut body, "Research", research);
    render_group(&mut body, "Articles", articles);
    replace_region(original, body.trim_end())
}

pub(crate) fn render_log(
    original: &str,
    date: &str,
    entries: &[LogEntry],
) -> Result<String, KbError> {
    let (_, managed, _) = region_parts(original)?;
    let lines = entries
        .iter()
        .map(|entry| {
            format!(
                "- **{}**: [{}]({}) — {}",
                if entry.creation { "Creation" } else { "Update" },
                escape_label(&entry.title),
                entry.path,
                entry.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let heading = format!("## {date}");
    let body = if managed.trim().is_empty() {
        format!("{heading}\n{lines}")
    } else if managed.trim_start().starts_with(&heading) {
        let leading = managed.len() - managed.trim_start().len();
        let normalized = &managed[leading..];
        let split = normalized.find('\n').unwrap_or(normalized.len());
        let after_heading = &normalized[split..];
        format!("{heading}\n{lines}{after_heading}")
    } else {
        format!("{heading}\n{lines}\n\n{}", managed.trim())
    };
    replace_region(original, body.trim_end())
}

fn render_group(output: &mut String, heading: &str, entries: &[IndexEntry]) {
    if entries.is_empty() {
        return;
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str("## ");
    output.push_str(heading);
    output.push('\n');
    for entry in entries {
        output.push_str("- [");
        output.push_str(&escape_label(&entry.title));
        output.push_str("](");
        output.push_str(&entry.path);
        output.push(')');
        if let Some(description) = &entry.description {
            output.push_str(" — ");
            output.push_str(&description.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        output.push('\n');
    }
}

fn replace_region(original: &str, managed: &str) -> Result<String, KbError> {
    let (before, _, after) = region_parts(original)?;
    let middle = if managed.is_empty() {
        String::new()
    } else {
        format!("\n{managed}")
    };
    Ok(format!("{before}{START}{middle}\n{END}{after}"))
}

fn region_parts(original: &str) -> Result<(&str, &str, &str), KbError> {
    let mut starts = original.match_indices(START);
    let Some((start, _)) = starts.next() else {
        return Err(markers());
    };
    if starts.next().is_some() {
        return Err(markers());
    }
    let after_start = start + START.len();
    let mut ends = original.match_indices(END);
    let Some((end, _)) = ends.next() else {
        return Err(markers());
    };
    if ends.next().is_some() || end < after_start {
        return Err(markers());
    }
    Ok((
        &original[..start],
        &original[after_start..end],
        &original[end + END.len()..],
    ))
}

fn markers() -> KbError {
    KbError::invalid_config(
        "managed Markdown region",
        "expected exactly one ordered kb:managed:start and kb:managed:end marker",
    )
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}
