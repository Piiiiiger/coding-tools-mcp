mod markdown;
mod model;
mod storage;

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::tools::context::ToolContext;
use crate::tools::workspace::{tool_ok, WorkspaceError, WorkspaceResult};

const PROJECT_STATE_RELATIVE_PATH: &str = "docs/history-state/PROJECT_STATE.md";
const MAX_PROJECT_STATE_BYTES: usize = 8 * 1024;
const MAX_LATEST_DELTA_BYTES: usize = 4 * 1024;

pub fn bootstrap(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let (session_key, source) = resolve_session_key(args)?;
    let host_session_key_mismatch = host_session_key(args)
        .map(|host| host != session_key.as_str())
        .unwrap_or(false);
    let history_dir = resolve_dir(ctx, args)?;
    storage::ensure_directory(&history_dir)?;
    let _lock = storage::lock_directory(&history_dir)?;
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    reject_ambiguous_history(&report)?;
    if !report.missing_numbers.is_empty() {
        return Err(history_error(
            "HISTORY_SEQUENCE_CONFLICT",
            "History numbering contains gaps; run history_session_validate before creating a session.",
            "validation",
            true,
            json!({"missing_numbers": report.missing_numbers}),
        ));
    }

    let mut warnings = Vec::<String>::new();
    if host_session_key_mismatch {
        warnings.push(
            "宿主会话标识与显式 session_key 不一致，已使用显式 session_key 保持会话连续。".into(),
        );
    }
    match storage::read_index(&history_dir) {
        Ok(Some(_)) => {}
        Ok(None) => warnings.push("历史索引缺失，已根据 Markdown 重建。".into()),
        Err(_) => warnings.push("历史索引损坏，已根据 Markdown 重建。".into()),
    }
    let readme = history_dir.join("README.md");
    if readme.exists() {
        fs::read_to_string(&readme).map_err(|error| {
            history_error(
                "HISTORY_READ_FAILED",
                &error.to_string(),
                "filesystem",
                true,
                json!({"path": "docs/history-session/README.md"}),
            )
        })?;
    } else {
        warnings.push("docs/history-session/README.md 不存在。".into());
    }

    let existing = report
        .documents
        .iter()
        .find(|document| document.session_key.as_deref() == Some(session_key.as_str()));
    let (current_number, current_path, created, resumed) = if let Some(document) = existing {
        (document.number, document.path.clone(), false, true)
    } else {
        if !args
            .get("create_if_missing")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            return Err(history_error(
                "SESSION_NOT_BOOTSTRAPPED",
                "No history mapping exists for this session_key.",
                "not_found",
                false,
                json!({"session_key_source": source}),
            ));
        }
        let number = report.latest_number().unwrap_or(0) + 1;
        let relative_path = format!("{}/{number}.md", history_dir_display(ctx, &history_dir));
        let timestamp = now_timestamp();
        let title = args
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("开发会话");
        let content = markdown::render_document(
            number,
            title,
            &session_key,
            &timestamp,
            &timestamp,
            "active",
            &[],
        );
        storage::write_markdown(&history_dir.join(format!("{number}.md")), &content)?;
        (number, relative_path, true, false)
    };

    let refreshed = storage::scan(&ctx.workspace, &history_dir)?;
    reject_ambiguous_history(&refreshed)?;
    storage::write_index(&history_dir, &storage::rebuild_index(&refreshed))?;

    let prior = report
        .documents
        .iter()
        .filter(|document| document.number != current_number)
        .collect::<Vec<_>>();
    let history_numbers = prior
        .iter()
        .map(|document| document.number)
        .collect::<Vec<_>>();
    let latest = prior.iter().max_by_key(|document| document.number).copied();
    let (project_state, project_state_truncated) = read_project_state(ctx)?;
    if project_state_truncated {
        warnings.push(format!(
            "{PROJECT_STATE_RELATIVE_PATH} 超过启动上下文预算，bootstrap 已返回截断版本；需要细节时按需读取原文件。"
        ));
    }
    let (latest_delta, latest_delta_truncated) = latest
        .map(|document| {
            bounded_utf8(
                &markdown::summary(&document.content),
                MAX_LATEST_DELTA_BYTES,
            )
        })
        .unwrap_or_else(|| (String::new(), false));
    if latest_delta_truncated {
        warnings.push(
            "最新会话摘要超过启动上下文预算，bootstrap 已返回截断版本；需要细节时按需读取 latest_completed_path。"
                .into(),
        );
    }
    let mut digest = Sha256::new();
    let mut total_bytes = 0_u64;
    for document in &prior {
        digest.update(document.number.to_le_bytes());
        digest.update(document.content.as_bytes());
        total_bytes += document.content.len() as u64;
    }

    Ok(tool_ok(json!({
        "is_new_session": created,
        "session_key": session_key.clone(),
        "session_key_source": source,
        "host_session_key_mismatch": host_session_key_mismatch,
        "history_numbers": history_numbers,
        "history_count": prior.len(),
        "latest_completed_number": latest.map(|document| document.number),
        "latest_completed_path": latest.map(|document| document.path.clone()),
        "current_number": current_number,
        "current_path": current_path.clone(),
        "created": created,
        "resumed": resumed,
        "sequence_valid": report.sequence_valid(),
        "project_state_path": project_state.as_ref().map(|_| PROJECT_STATE_RELATIVE_PATH),
        "project_state": project_state.unwrap_or_default(),
        "project_state_truncated": project_state_truncated,
        "latest_delta": latest_delta,
        "latest_delta_truncated": latest_delta_truncated,
        "history_read_mode": "project_state_plus_latest_delta",
        "total_history_bytes": total_bytes,
        "full_history_included": false,
        "history_digest": format!("{:x}", digest.finalize()),
        "persistence_mode": "model_mediated_tool_calls",
        "assistant_instructions": "Use project_state as the canonical rolling context and latest_delta as the bounded recent handoff. Do not expand older history automatically; when a task needs historical detail, read latest_completed_path or a specific numbered history file on demand. Preserve the session_key and current_path returned by bootstrap, then pass them unchanged as session_key and expected_path to every history_session_checkpoint call. After completing each user-requested task, call history_session_checkpoint before the final response. Only state that progress was saved after checkpoint returns ok=true with the same session_key and path.",
        "required_next_actions": [
            "use_project_state_and_latest_delta",
            "verify_workspace_state",
            "execute_user_task",
            "checkpoint_after_each_completed_task"
        ],
        "checkpoint_policy": {
            "tool": "history_session_checkpoint",
            "session_key": session_key,
            "expected_path": current_path,
            "stable_target_required": true,
            "required_before_final_response": true,
            "applies_after_bootstrap": true,
            "automatic_background_persistence": false
        },
        "warnings": warnings
    })))
}

fn host_session_key(args: &Value) -> Option<&str> {
    args.get("_host_session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required_checkpoint_argument(args: &Value, name: &str) -> WorkspaceResult<String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            history_error(
                "CHECKPOINT_TARGET_REQUIRED",
                "Pass session_key and expected_path exactly as returned by history_session_bootstrap.",
                "validation",
                false,
                json!({"missing_argument": name}),
            )
        })
}

pub fn checkpoint(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let session_key = required_checkpoint_argument(args, "session_key")?;
    let expected_path = required_checkpoint_argument(args, "expected_path")?;
    let host_session_key_mismatch = host_session_key(args)
        .map(|host| host != session_key.as_str())
        .unwrap_or(false);
    let history_dir = resolve_dir(ctx, args)?;
    if !history_dir.exists() {
        return Err(session_not_bootstrapped());
    }
    let _lock = storage::lock_directory(&history_dir)?;
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    reject_ambiguous_history(&report)?;
    let document = report
        .documents
        .iter()
        .find(|document| document.session_key.as_deref() == Some(session_key.as_str()))
        .ok_or_else(session_not_bootstrapped)?;
    if document.path != expected_path {
        return Err(history_error(
            "SESSION_TARGET_MISMATCH",
            "The checkpoint target does not match the session initialized by bootstrap.",
            "validation",
            false,
            json!({
                "expected_path": expected_path,
                "resolved_path": document.path,
                "session_key": session_key
            }),
        ));
    }

    let timestamp = now_timestamp();
    let mut record = markdown::checkpoint_from_args(args, &timestamp)
        .map_err(WorkspaceError::invalid_argument)?;
    let redacted = markdown::redact_record(&mut record);
    let mut records = markdown::parse_checkpoint_records(&document.content);
    let mut duplicate_ignored = false;
    let mut updated = false;
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.turn_id == record.turn_id)
    {
        if existing == &record {
            duplicate_ignored = true;
        } else {
            *existing = record.clone();
            updated = true;
        }
    } else {
        records.push(record.clone());
        updated = true;
    }

    let final_content = if duplicate_ignored {
        document.content.clone()
    } else {
        let created_at = document
            .created_at
            .clone()
            .unwrap_or_else(|| timestamp.clone());
        markdown::render_document(
            document.number,
            &markdown::document_title(&document.content, document.number),
            &session_key,
            &created_at,
            &record.timestamp,
            "active",
            &records,
        )
    };
    if !duplicate_ignored {
        storage::write_markdown(
            &history_dir.join(format!("{}.md", document.number)),
            &final_content,
        )?;
        let refreshed = storage::scan(&ctx.workspace, &history_dir)?;
        storage::write_index(&history_dir, &storage::rebuild_index(&refreshed))?;
    }
    let mut warnings = Vec::new();
    if redacted {
        warnings.push("检测到疑似敏感信息，归档内容已脱敏。");
    }
    if host_session_key_mismatch {
        warnings.push("宿主会话标识已变化；本次仍使用 bootstrap 返回的稳定目标，未切换历史文件。");
    }
    Ok(tool_ok(json!({
        "session_number": document.number,
        "path": document.path,
        "session_key": session_key,
        "expected_path": expected_path,
        "host_session_key_mismatch": host_session_key_mismatch,
        "turn_id": record.turn_id,
        "created": false,
        "updated": updated,
        "duplicate_ignored": duplicate_ignored,
        "content_hash": storage::sha256(final_content.as_bytes()),
        "warnings": warnings
    })))
}

pub fn validate(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let history_dir = resolve_dir(ctx, args)?;
    let repair = args.get("repair").and_then(Value::as_bool).unwrap_or(false);
    if repair {
        storage::ensure_directory(&history_dir)?;
    }
    let mut index_status = "missing";
    if history_dir.exists() {
        index_status = match storage::read_index(&history_dir) {
            Ok(Some(_)) => "valid",
            Ok(None) => "missing",
            Err(_) => "invalid",
        };
    }
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    let mut warnings = Vec::<String>::new();
    if !report.duplicate_session_keys.is_empty() {
        warnings.push("存在重复 session_key，相关映射未写入索引。".into());
    }
    let repaired = if repair {
        let _lock = storage::lock_directory(&history_dir)?;
        let locked_report = storage::scan(&ctx.workspace, &history_dir)?;
        storage::write_index(&history_dir, &storage::rebuild_index(&locked_report))?;
        true
    } else {
        false
    };
    let latest_number = report.latest_number();
    let latest_path = latest_number.and_then(|number| {
        report
            .documents
            .iter()
            .find(|document| document.number == number)
            .map(|document| document.path.clone())
    });
    Ok(tool_ok(json!({
        "sequence_valid": report.sequence_valid(),
        "numbers": report.numbers,
        "missing_numbers": report.missing_numbers,
        "duplicate_session_keys": report.duplicate_session_keys,
        "invalid_files": report.invalid_files,
        "empty_files": report.empty_files,
        "latest_number": latest_number,
        "latest_path": latest_path,
        "index_status": index_status,
        "repaired": repaired,
        "warnings": warnings
    })))
}

fn resolve_dir(ctx: &ToolContext, args: &Value) -> WorkspaceResult<std::path::PathBuf> {
    storage::resolve_history_dir(
        &ctx.workspace,
        args.get("workspace_root").and_then(Value::as_str),
        args.get("history_dir").and_then(Value::as_str),
    )
}

fn resolve_session_key(args: &Value) -> WorkspaceResult<(String, &'static str)> {
    if let Some(value) = args
        .get("session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok((value.to_string(), "explicit_session_key"));
    }
    if let Some(value) = args
        .get("_host_session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok((value.to_string(), "platform_conversation_id"));
    }
    Err(history_error(
        "SESSION_ID_UNAVAILABLE",
        "A stable ChatGPT session identifier is required.",
        "validation",
        false,
        json!({}),
    ))
}

fn reject_ambiguous_history(report: &model::ScanReport) -> WorkspaceResult<()> {
    if report.duplicate_session_keys.is_empty() {
        return Ok(());
    }
    Err(history_error(
        "HISTORY_INDEX_CONFLICT",
        "Multiple history files declare the same session_key.",
        "validation",
        false,
        json!({"duplicate_session_keys": report.duplicate_session_keys}),
    ))
}

fn session_not_bootstrapped() -> WorkspaceError {
    history_error(
        "SESSION_NOT_BOOTSTRAPPED",
        "The session_key has not been bootstrapped.",
        "not_found",
        false,
        json!({}),
    )
}

fn history_error(
    code: &'static str,
    message: &str,
    category: &'static str,
    retryable: bool,
    details: Value,
) -> WorkspaceError {
    WorkspaceError::ToolDetails {
        code,
        message: message.into(),
        category,
        retryable,
        details,
    }
}

fn history_dir_display(ctx: &ToolContext, path: &std::path::Path) -> String {
    crate::tools::workspace::relative_display(ctx.workspace.root(), path)
}

fn now_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{seconds}")
}

fn read_project_state(ctx: &ToolContext) -> WorkspaceResult<(Option<String>, bool)> {
    let path = ctx.workspace.root().join(PROJECT_STATE_RELATIVE_PATH);
    if !path.is_file() {
        return Ok((None, false));
    }
    let content = fs::read_to_string(&path).map_err(|error| {
        history_error(
            "HISTORY_READ_FAILED",
            &error.to_string(),
            "filesystem",
            true,
            json!({"path": PROJECT_STATE_RELATIVE_PATH}),
        )
    })?;
    let (bounded, truncated) = bounded_utf8(&content, MAX_PROJECT_STATE_BYTES);
    Ok((Some(bounded), truncated))
}

fn bounded_utf8(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    const SUFFIX: &str = "\n…（启动上下文已截断）";
    let mut end = max_bytes.saturating_sub(SUFFIX.len()).min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = value[..end].to_string();
    truncated.push_str(SUFFIX);
    (truncated, true)
}
