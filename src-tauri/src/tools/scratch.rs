use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SCRATCH_RELATIVE_ROOT: &str = ".coding-tools/scratch";
const SCRATCH_TTL: Duration = Duration::from_secs(48 * 60 * 60);

pub struct ScratchDir {
    path: PathBuf,
    available: bool,
}

impl ScratchDir {
    pub fn initialize(workspace_root: &Path) -> Self {
        let scratch_root = workspace_root.join(SCRATCH_RELATIVE_ROOT);
        let available = fs::create_dir_all(&scratch_root).is_ok();

        if available {
            cleanup_stale_entries(&scratch_root, SCRATCH_TTL);
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = scratch_root.join(format!(
            "session-{}-{now_ms}",
            std::process::id()
        ));
        let available = available && fs::create_dir_all(&path).is_ok();

        Self { path, available }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn available(&self) -> bool {
        self.available && self.path.is_dir()
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn cleanup_stale_entries(scratch_root: &Path, ttl: Duration) {
    let Ok(entries) = fs::read_dir(scratch_root) else {
        return;
    };
    let now = SystemTime::now();

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age < ttl {
            continue;
        }

        if metadata.file_type().is_symlink() || metadata.is_file() {
            let _ = fs::remove_file(&path);
        } else if metadata.is_dir() {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

pub fn looks_like_transient_root_helper(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    if normalized.contains('/') {
        return false;
    }

    let lower = normalized.to_ascii_lowercase();
    let script_like = [".py", ".js", ".ts", ".sh", ".ps1"]
        .iter()
        .any(|suffix| lower.ends_with(suffix));
    if !script_like {
        return false;
    }

    lower.starts_with("tmp_")
        || lower.starts_with("temp_")
        || lower.starts_with("debug_")
        || lower.starts_with("inspect_")
        || lower.starts_with("scratch_")
        || lower.contains("_probe.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_dir_is_created_under_hidden_workspace_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scratch = ScratchDir::initialize(temp.path());

        assert!(scratch.available());
        assert!(scratch.path().starts_with(temp.path().join(SCRATCH_RELATIVE_ROOT)));
    }

    #[test]
    fn detects_obvious_transient_root_helpers_only() {
        assert!(looks_like_transient_root_helper("tmp_probe.py"));
        assert!(looks_like_transient_root_helper("weston_keyboard_probe.py"));
        assert!(looks_like_transient_root_helper("inspect_current_ns.py"));
        assert!(!looks_like_transient_root_helper("scripts/check_health.py"));
        assert!(!looks_like_transient_root_helper("build.py"));
        assert!(!looks_like_transient_root_helper("README.md"));
    }
}
