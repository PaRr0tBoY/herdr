//! herdr → hive data directory migration.
//!
//! On first Hive boot, if the new `hive` config/state directory does not exist
//! but an old `herdr` directory does, this module copies the old data into
//! the new locations so existing sessions, config, and plugins are preserved.

use std::path::{Path, PathBuf};

/// Old app directory name before the rename.
fn old_app_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "herdr-dev"
    } else {
        "herdr"
    }
}

/// The old config directory path (XDG or platform fallback).
fn old_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join(old_app_dir_name());
    }
    #[cfg(windows)]
    {
        if let Ok(dir) = std::env::var("APPDATA") {
            return PathBuf::from(dir).join(old_app_dir_name());
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile)
                .join("AppData")
                .join("Roaming")
                .join(old_app_dir_name());
        }
    }
    #[cfg(not(windows))]
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(format!(".config/{}", old_app_dir_name()));
    }
    std::env::temp_dir().join(old_app_dir_name())
}

/// The old state directory path (XDG or platform fallback).
fn old_state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(dir).join(old_app_dir_name());
    }
    #[cfg(windows)]
    {
        if let Ok(dir) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(dir).join(old_app_dir_name());
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile)
                .join("AppData")
                .join("Local")
                .join(old_app_dir_name());
        }
    }
    #[cfg(not(windows))]
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(format!(".local/state/{}", old_app_dir_name()));
    }
    std::env::temp_dir().join(format!("{}-state", old_app_dir_name()))
}

/// Run the migration once: copy old `herdr` config/state to new `hive` paths if
/// the new directory does not yet exist and the old one does.
///
/// Should be called once at startup, before config or session data is loaded.
pub fn migrate_from_herdr() {
    let new_config = crate::config::config_dir();
    let old_config = old_config_dir();

    if new_config.exists() {
        tracing::debug!(path = %new_config.display(), "hive config dir already exists, skipping migration");
        return;
    }

    if old_config.exists() {
        tracing::info!(
            from = %old_config.display(),
            to = %new_config.display(),
            "migrating config data from herdr to hive"
        );
        if let Err(err) = copy_dir(&old_config, &new_config) {
            tracing::error!(
                from = %old_config.display(),
                to = %new_config.display(),
                error = %err,
                "failed to migrate config data from herdr"
            );
            // Leave the partial copy for the user to clean up.
            return;
        }
        tracing::info!(path = %new_config.display(), "config data migrated");
    }

    // Also migrate state directory (e.g., `.local/state/herdr` → `.local/state/hive`).
    let new_state = crate::config::state_dir();
    let old_state = old_state_dir();
    if new_state.exists() {
        return;
    }
    if old_state.exists() {
        tracing::info!(
            from = %old_state.display(),
            to = %new_state.display(),
            "migrating state data from herdr to hive"
        );
        if let Err(err) = copy_dir(&old_state, &new_state) {
            tracing::error!(
                from = %old_state.display(),
                to = %new_state.display(),
                error = %err,
                "failed to migrate state data from herdr"
            );
            return;
        }
        tracing::info!(path = %new_state.display(), "state data migrated");
    }
}

/// Recursively copy a directory tree.
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let name = entry.file_name();
        let dst_path = dst.join(&name);

        if file_type.is_dir() {
            copy_dir(&src_path, &dst_path)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            // Use copy (not symlink) so the old herdr dir can be safely removed later.
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_app_dir_name_is_herdr() {
        // The old name should always be "herdr" regardless of debug/release.
        let name = old_app_dir_name();
        assert!(
            name.starts_with("herdr"),
            "expected herdr prefix, got {name}"
        );
    }

    #[test]
    fn copy_dir_copies_all_files() {
        let tmp = std::env::temp_dir().join(format!("hive-migrate-test-{}", std::process::id()));
        let src = tmp.join("herdr");
        let dst = tmp.join("hive");

        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(src.join("sessions/default")).unwrap();
        std::fs::create_dir_all(src.join("plugins/example")).unwrap();
        std::fs::write(src.join("config.toml"), "key = \"value\"\n").unwrap();
        std::fs::write(src.join("sessions/default/hive.sock"), "stub").unwrap();
        std::fs::write(src.join("plugins/example/manifest.toml"), "id = \"test\"\n").unwrap();

        copy_dir(&src, &dst).unwrap();

        assert!(dst.join("config.toml").exists());
        assert!(dst.join("sessions/default/hive.sock").exists());
        assert!(dst.join("plugins/example/manifest.toml").exists());
        assert_eq!(
            std::fs::read_to_string(dst.join("config.toml")).unwrap(),
            "key = \"value\"\n"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn copy_dir_creates_parent_dirs() {
        let tmp =
            std::env::temp_dir().join(format!("hive-migrate-parent-test-{}", std::process::id()));
        let src = tmp.join("herdr");
        let dst = tmp.join("nested/hive");

        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("config.toml"), "x = 1\n").unwrap();

        copy_dir(&src, &dst).unwrap();
        assert!(dst.join("config.toml").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
