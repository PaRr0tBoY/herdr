use std::path::PathBuf;

/// Returns the directory where per-workspace floating note files are stored.
///
/// - Windows: `%APPDATA%\herdr\floating-notes\`
/// - Unix: `~/.local/share/herdr/floating-notes/`
pub(crate) fn note_dir() -> PathBuf {
    crate::config::state_dir().join("floating-notes")
}

/// Returns the note file path for a given workspace id.
pub(crate) fn note_path(workspace_id: &str) -> PathBuf {
    note_dir().join(format!("{workspace_id}.md"))
}

/// Deletes the note file for a workspace, if it exists. Ignores errors.
pub(crate) fn delete_note_file(workspace_id: &str) {
    let path = note_path(workspace_id);
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_path_contains_workspace_id() {
        let path = note_path("test-ws-123");
        assert!(path.to_string_lossy().contains("test-ws-123"));
        assert!(path.to_string_lossy().ends_with(".md"));
    }

    #[test]
    fn note_dir_contains_floating_notes() {
        let dir = note_dir();
        assert!(dir.to_string_lossy().contains("floating-notes"));
    }
}
