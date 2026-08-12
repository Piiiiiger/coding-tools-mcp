use serde_json::Value;

/// Hard ceiling for model-facing text emitted by one MCP tool result.
///
/// Large tool bodies remain recoverable through the tool's own paging/range
/// arguments. Keeping this small prevents a handful of read/exec/git calls
/// from consuming an entire ChatGPT conversation context.
pub(crate) const MODEL_TEXT_SAFETY_LIMIT_BYTES: usize = 4 * 1024;

pub fn render_tool_text(tool_name: &str, payload: &Value, is_error: bool) -> String {
    let rendered = if is_error || payload.get("ok").and_then(Value::as_bool) == Some(false) {
        render_error(payload)
    } else {
        match tool_name {
            "server_info" => render_server_info(payload),
            "history_session_bootstrap" => render_history_bootstrap(payload),
            "history_session_checkpoint" => render_history_checkpoint(payload),
            "history_session_validate" => render_history_validate(payload),
            "history_session_search" => render_history_search(payload),
            "history_session_read" => render_history_read(payload),
            "check_exec_environment" => render_exec_environment(payload),
            "get_default_cwd" | "set_default_cwd" => render_cwd(payload),
            "read_file" => render_read_file(payload),
            "list_dir" | "list_files" => render_list(payload),
            "search_text" | "grep_text" => render_search(payload),
            "apply_patch" => render_patch(payload),
            "exec_command" | "write_stdin" => render_exec(payload),
            "kill_session" => render_kill(payload),
            "read_output" => render_read_output(payload),
            "git_status" => render_git_status(payload),
            "git_diff" => render_key(payload, "diff", "No diff."),
            "git_log" => render_git_log(payload),
            "git_show" => render_key(payload, "content", "No output."),
            "git_blame" => render_git_blame(payload),
            "request_permissions" => format!(
                "Permission request: {}.",
                string_value(payload, "status").unwrap_or("completed")
            ),
            "view_image" => render_image(payload),
            _ => payload
                .get("summary")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "{tool_name}: {}.",
                        string_value(payload, "status").unwrap_or("completed")
                    )
                }),
        }
    };
    bounded_model_text(&rendered, tool_name)
}

fn render_history_search(payload: &Value) -> String {
    let query = string_value(payload, "query").unwrap_or("");
    let total = payload
        .get("total_matches")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cursor = payload.get("cursor").and_then(Value::as_u64).unwrap_or(0);
    let next_cursor = payload
        .get("next_cursor")
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".into());
    let mut lines = vec![format!(
        "History search: query={query:?}; total={total}; cursor={cursor}; next_cursor={next_cursor}."
    )];
    if let Some(results) = payload.get("results").and_then(Value::as_array) {
        for result in results {
            let number = result.get("number").and_then(Value::as_u64).unwrap_or(0);
            let path = string_value(result, "path").unwrap_or("unknown");
            let title = string_value(result, "title").unwrap_or("");
            let snippet = string_value(result, "snippet").unwrap_or("");
            lines.push(format!("#{number} {path} — {title}\n{snippet}"));
        }
    }
    lines.join("\n")
}

fn render_history_read(payload: &Value) -> String {
    let number = payload.get("number").and_then(Value::as_u64).unwrap_or(0);
    let path = string_value(payload, "path").unwrap_or("unknown");
    let cursor = payload.get("cursor").and_then(Value::as_u64).unwrap_or(0);
    let next_cursor = payload
        .get("next_cursor")
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".into());
    let total_bytes = payload
        .get("total_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let content_hash = string_value(payload, "content_hash").unwrap_or("unknown");
    let content = string_value(payload, "content").unwrap_or("");
    format!(
        "History archive #{number}: {path}\ncursor={cursor}; next_cursor={next_cursor}; total_bytes={total_bytes}; content_hash={content_hash}\n{content}"
    )
}

fn render_error(payload: &Value) -> String {
    let error = payload.get("error").unwrap_or(&Value::Null);
    let code = string_value(error, "code").unwrap_or("TOOL_ERROR");
    let message = string_value(error, "message")
        .or_else(|| string_value(payload, "summary"))
        .unwrap_or("Tool call failed.");
    let mut lines = vec![format!("{code}: {message}")];
    if let Some(retry) = error
        .get("details")
        .and_then(|details| details.get("retry_hint"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("Retry: {retry}"));
    }
    lines.join("\n")
}

fn render_server_info(payload: &Value) -> String {
    format!(
        "{} {}\nWorkspace: {}",
        string_value(payload, "server").unwrap_or("coding-tools-mcp"),
        string_value(payload, "version").unwrap_or("unknown"),
        string_value(payload, "workspace").unwrap_or(".")
    )
}

fn render_history_bootstrap(payload: &Value) -> String {
    let count = payload
        .get("history_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let current = string_value(payload, "current_path").unwrap_or("unknown");
    let mode = string_value(payload, "history_read_mode").unwrap_or("compact");
    let state = string_value(payload, "project_state_path").unwrap_or("not present");
    format!(
        "History initialized: {count} prior session(s); current handoff: {current}.\nContext mode: {mode}; canonical state: {state}. Older history is available on demand by path."
    )
}

fn render_history_checkpoint(payload: &Value) -> String {
    format!(
        "History checkpoint saved: {} (turn {}).",
        string_value(payload, "path").unwrap_or("unknown"),
        string_value(payload, "turn_id").unwrap_or("unknown")
    )
}

fn render_history_validate(payload: &Value) -> String {
    let valid = payload
        .get("sequence_valid")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let latest = payload
        .get("latest_number")
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!(
        "History validation: {}; latest session: {latest}; index: {}.",
        if valid { "PASS" } else { "FAIL" },
        string_value(payload, "index_status").unwrap_or("unknown")
    )
}

fn render_exec_environment(payload: &Value) -> String {
    let sandbox = payload
        .get("filesystem_sandbox")
        .and_then(|value| value.get("enforced"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let boundary = string_value(payload, "workspace_exec_boundary").unwrap_or("unknown");
    format!(
        "Execution environment checked. Filesystem sandbox: {}; execution boundary: {boundary}.",
        if sandbox { "enforced" } else { "not enforced" }
    )
}

fn render_cwd(payload: &Value) -> String {
    format!(
        "Default working directory: {}",
        string_value(payload, "default_cwd").unwrap_or(".")
    )
}

fn render_read_file(payload: &Value) -> String {
    let Some(content) = string_value(payload, "content") else {
        return String::new();
    };
    if payload.get("truncated").and_then(Value::as_bool) != Some(true) {
        return content.to_string();
    }
    format!(
        "[Showing lines {}-{} of {}; content truncated, request a narrower range or continue from the returned line metadata.]\n{content}",
        display_scalar(payload.get("start_line")),
        display_scalar(payload.get("end_line")),
        display_scalar(payload.get("total_lines"))
    )
}

fn render_list(payload: &Value) -> String {
    let entries = payload
        .get("entries")
        .or_else(|| payload.get("files"))
        .and_then(Value::as_array);
    let Some(entries) = entries else {
        return "No entries found.".to_string();
    };
    if entries.is_empty() {
        return "No entries found.".to_string();
    }
    let mut lines = Vec::new();
    for item in entries {
        let path = string_value(item, "path")
            .or_else(|| string_value(item, "name"))
            .unwrap_or("");
        let kind = string_value(item, "type");
        lines.push(match kind {
            Some(kind) => format!("{path} [{kind}]"),
            None => path.to_string(),
        });
    }
    if payload.get("truncated").and_then(Value::as_bool) == Some(true) {
        lines
            .push("… results truncated; narrow the path/patterns or raise the entry limit.".into());
    }
    lines.join("\n")
}

fn render_search(payload: &Value) -> String {
    let Some(matches) = payload.get("matches").and_then(Value::as_array) else {
        return "No matches found.".to_string();
    };
    if matches.is_empty() {
        return "No matches found.".to_string();
    }
    let mut lines = Vec::new();
    for item in matches {
        let path = string_value(item, "path").unwrap_or("");
        let line = display_scalar(item.get("line"));
        let column = display_scalar(item.get("column"));
        let preview = string_value(item, "preview").unwrap_or("");
        let location = if column.is_empty() || column == "?" {
            format!("{path}:{line}")
        } else {
            format!("{path}:{line}:{column}")
        };
        lines.push(format!("{location}: {preview}"));
        for key in ["before", "after"] {
            if let Some(context) = item.get(key).and_then(Value::as_array) {
                lines.extend(
                    context
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|value| format!("  {value}")),
                );
            }
        }
    }
    if payload.get("truncated").and_then(Value::as_bool) == Some(true) {
        lines.push("… results truncated; narrow the query/path or raise max_results.".into());
    }
    lines.join("\n")
}

fn render_patch(payload: &Value) -> String {
    let dry_run = payload.get("dry_run").and_then(Value::as_bool) == Some(true);
    let count = payload
        .get("affected_files")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let summary = string_value(payload, "summary").unwrap_or("");
    let mut text = format!(
        "Patch {} for {count} file(s).",
        if dry_run { "validated" } else { "applied" }
    );
    if !summary.is_empty() {
        text.push('\n');
        text.push_str(summary);
    }
    text
}

fn render_exec(payload: &Value) -> String {
    let mut header = vec![format!(
        "Status: {}",
        string_value(payload, "status").unwrap_or("unknown")
    )];
    if let Some(exit_code) = payload.get("exit_code").and_then(Value::as_i64) {
        header.push(format!("exit code {exit_code}"));
    }
    if let Some(signal) = string_value(payload, "signal") {
        header.push(format!("signal {signal}"));
    }
    if payload.get("timed_out").and_then(Value::as_bool) == Some(true) {
        header.push("timed out".into());
    }
    if let Some(ms) = payload.get("elapsed_ms").and_then(Value::as_u64) {
        header.push(format!("{ms} ms"));
    }
    let mut sections = vec![header.join(" | ")];
    if let Some(stdout) = string_value(payload, "stdout").filter(|value| !value.is_empty()) {
        sections.push(stdout.to_string());
    }
    if let Some(stderr) = string_value(payload, "stderr").filter(|value| !value.is_empty()) {
        sections.push(format!("stderr:\n{stderr}"));
    }
    if sections.len() == 1 {
        if let Some(preview) = string_value(payload, "preview").filter(|value| !value.is_empty()) {
            sections.push(preview.to_string());
        }
    }
    if payload.get("status").and_then(Value::as_str) == Some("running") {
        if let Some(session_id) = string_value(payload, "session_id") {
            sections.push(format!(
                "Session still running; poll with write_stdin(session_id=\"{session_id}\", chars=\"\")."
            ));
        }
    }
    if payload.get("truncated").and_then(Value::as_bool) == Some(true)
        || payload.get("stdout_truncated").and_then(Value::as_bool) == Some(true)
        || payload.get("stderr_truncated").and_then(Value::as_bool) == Some(true)
    {
        sections.push(
            "Output truncated; use read_output with the returned output_ref to read more.".into(),
        );
    }
    sections.join("\n")
}

fn render_read_output(payload: &Value) -> String {
    let content = string_value(payload, "content")
        .or_else(|| string_value(payload, "stdout"))
        .unwrap_or("");
    if payload.get("next_offset").is_some() {
        format!(
            "{content}\n[more output is available; continue with read_output using next_offset]"
        )
    } else {
        content.to_string()
    }
}

fn render_kill(payload: &Value) -> String {
    format!(
        "Session {}: {}.",
        string_value(payload, "session_id").unwrap_or("unknown"),
        string_value(payload, "status").unwrap_or("completed")
    )
}

fn render_git_status(payload: &Value) -> String {
    if payload.get("is_repo").and_then(Value::as_bool) == Some(false) {
        return "Not a Git repository.".to_string();
    }
    let mut lines = vec![format!(
        "## {}",
        string_value(payload, "branch").unwrap_or("detached")
    )];
    if let Some(entries) = payload.get("entries").and_then(Value::as_array) {
        for entry in entries {
            lines.push(format!(
                "{}{} {}",
                string_value(entry, "index_status").unwrap_or(" "),
                string_value(entry, "worktree_status").unwrap_or(" "),
                string_value(entry, "path").unwrap_or("")
            ));
        }
        if entries.is_empty() {
            lines.push("Working tree clean.".into());
        }
    }
    lines.join("\n")
}

fn render_git_log(payload: &Value) -> String {
    let Some(commits) = payload.get("commits").and_then(Value::as_array) else {
        return "No commits found.".to_string();
    };
    if commits.is_empty() {
        return "No commits found.".to_string();
    }
    commits
        .iter()
        .map(|item| {
            format!(
                "{} {}",
                string_value(item, "short_hash").unwrap_or(""),
                string_value(item, "subject").unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_git_blame(payload: &Value) -> String {
    let Some(lines) = payload.get("lines").and_then(Value::as_array) else {
        return "No blame lines found.".to_string();
    };
    lines
        .iter()
        .map(|item| {
            format!(
                "{} {} {}",
                display_scalar(item.get("line")),
                string_value(item, "commit").unwrap_or(""),
                string_value(item, "content").unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_image(payload: &Value) -> String {
    format!(
        "Image: {} ({})",
        string_value(payload, "path").unwrap_or(""),
        string_value(payload, "mime_type").unwrap_or("unknown")
    )
}

fn render_key(payload: &Value, key: &str, empty: &str) -> String {
    string_value(payload, key)
        .filter(|value| !value.is_empty())
        .unwrap_or(empty)
        .to_string()
}

fn string_value<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn display_scalar(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => "?".to_string(),
    }
}

fn bounded_model_text(value: &str, tool_name: &str) -> String {
    if value.len() <= MODEL_TEXT_SAFETY_LIMIT_BYTES {
        return value.to_string();
    }
    let suffix = format!(
        "\n… {tool_name} model text reached the {MODEL_TEXT_SAFETY_LIMIT_BYTES}-byte safety ceiling; retry with narrower paths or limits."
    );
    let mut end = MODEL_TEXT_SAFETY_LIMIT_BYTES.saturating_sub(suffix.len());
    end = end.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut output = value[..end].to_string();
    output.push_str(&suffix);
    output
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{render_tool_text, MODEL_TEXT_SAFETY_LIMIT_BYTES};

    #[test]
    fn history_bootstrap_text_does_not_mirror_structured_history() {
        let payload = json!({
            "ok": true,
            "history_count": 42,
            "current_path": "docs/history-session/43.md",
            "history_read_mode": "project_state_plus_latest_delta",
            "project_state_path": "docs/history-state/PROJECT_STATE.md",
            "project_state": "VERY_LARGE_PRIVATE_STATE_SHOULD_STAY_STRUCTURED",
            "latest_delta": "VERY_LARGE_LATEST_DELTA_SHOULD_STAY_STRUCTURED"
        });
        let text = render_tool_text("history_session_bootstrap", &payload, false);
        assert!(text.contains("42 prior session"));
        assert!(!text.contains("VERY_LARGE_PRIVATE_STATE_SHOULD_STAY_STRUCTURED"));
        assert!(!text.contains("VERY_LARGE_LATEST_DELTA_SHOULD_STAY_STRUCTURED"));
    }

    #[test]
    fn large_read_file_model_text_is_bounded() {
        let payload = json!({
            "ok": true,
            "path": "large.txt",
            "content": "界".repeat(20_000),
            "start_line": 1,
            "end_line": 1,
            "total_lines": 1,
            "truncated": false
        });

        let text = render_tool_text("read_file", &payload, false);

        assert!(text.len() <= MODEL_TEXT_SAFETY_LIMIT_BYTES);
        assert!(text.contains("model text reached"));
        assert!(text.contains("retry with narrower paths or limits"));
    }

    #[test]
    fn history_read_model_text_keeps_page_content_and_continuation_cursor() {
        let payload = json!({
            "ok": true,
            "number": 20,
            "path": "docs/history-session/20.md",
            "content": "历史正文 marker\n".repeat(400),
            "cursor": 4096,
            "next_cursor": 8192,
            "total_bytes": 12000,
            "content_hash": "sha256:test"
        });

        let text = render_tool_text("history_session_read", &payload, false);

        assert!(text.len() <= MODEL_TEXT_SAFETY_LIMIT_BYTES);
        assert!(text.contains("历史正文 marker"));
        assert!(text.contains("next_cursor=8192"));
        assert!(text.contains("content_hash=sha256:test"));
    }

    #[test]
    fn history_search_model_text_keeps_hits_and_next_cursor() {
        let payload = json!({
            "ok": true,
            "query": "FG-350",
            "total_matches": 3,
            "cursor": 0,
            "next_cursor": 2,
            "results": [{
                "number": 20,
                "path": "docs/history-session/20.md",
                "title": "Read current project progress",
                "snippet": "FG-350 exact CGF verification"
            }]
        });

        let text = render_tool_text("history_session_search", &payload, false);

        assert!(text.contains("next_cursor=2"));
        assert!(text.contains("docs/history-session/20.md"));
        assert!(text.contains("FG-350 exact CGF verification"));
    }
}
