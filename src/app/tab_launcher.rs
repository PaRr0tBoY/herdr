//! Tab launcher types, discovery, and agent name sync logic.

use crate::detect::Agent;

/// Check whether a command exists on PATH.
fn binary_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .any(|dir| dir.join(name).exists() || dir.join(format!("{name}.exe")).exists())
        })
        .unwrap_or(false)
}

/// A single item in the tab launcher.
#[derive(Debug, Clone)]
pub struct LauncherItem {
    pub label: String,
    pub detail: String,
    pub kind: LauncherItemKind,
}

/// What kind of tab a launcher item creates.
#[derive(Debug, Clone)]
pub enum LauncherItemKind {
    /// Create a shell tab with the given program.
    Shell { program: String },
    /// Create an agent tab with the given argv.
    Agent { argv: Vec<String> },
}

/// A category of launcher items.
#[derive(Debug, Clone)]
pub struct LauncherCategory {
    pub name: String,
    pub items: Vec<LauncherItem>,
}

/// Discover available launcher items from the system.
pub fn discover_launcher_items() -> Vec<LauncherCategory> {
    let mut categories = Vec::new();

    let shells = discover_shells();
    if !shells.is_empty() {
        categories.push(LauncherCategory {
            name: "Shell".into(),
            items: shells,
        });
    }

    let agents = discover_agents();
    if !agents.is_empty() {
        categories.push(LauncherCategory {
            name: "AI Agent".into(),
            items: agents,
        });
    }

    categories
}

#[cfg(windows)]
fn discover_shells() -> Vec<LauncherItem> {
    let mut items = Vec::new();

    if binary_on_path("pwsh.exe") {
        items.push(LauncherItem {
            label: "PowerShell 7".into(),
            detail: "pwsh.exe".into(),
            kind: LauncherItemKind::Shell {
                program: "pwsh.exe".into(),
            },
        });
    }

    if binary_on_path("powershell.exe") {
        items.push(LauncherItem {
            label: "Windows PowerShell".into(),
            detail: "powershell.exe".into(),
            kind: LauncherItemKind::Shell {
                program: "powershell.exe".into(),
            },
        });
    }

    items.push(LauncherItem {
        label: "Command Prompt".into(),
        detail: "cmd.exe".into(),
        kind: LauncherItemKind::Shell {
            program: "cmd.exe".into(),
        },
    });

    if binary_on_path("wsl.exe") {
        items.push(LauncherItem {
            label: "WSL".into(),
            detail: "wsl.exe".into(),
            kind: LauncherItemKind::Shell {
                program: "wsl.exe".into(),
            },
        });
    }

    items
}

#[cfg(unix)]
fn discover_shells() -> Vec<LauncherItem> {
    let mut items = Vec::new();

    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.is_empty() {
            items.push(LauncherItem {
                label: format!("Default ({})", shell),
                detail: shell.clone(),
                kind: LauncherItemKind::Shell { program: shell },
            });
        }
    }

    for candidate in &["/bin/zsh", "/bin/bash", "/bin/fish", "/bin/sh"] {
        if std::path::Path::new(candidate).exists() {
            let name = candidate.trim_start_matches("/bin/");
            items.push(LauncherItem {
                label: name.to_string(),
                detail: candidate.to_string(),
                kind: LauncherItemKind::Shell {
                    program: candidate.to_string(),
                },
            });
        }
    }

    items
}

fn discover_agents() -> Vec<LauncherItem> {
    let mut items = Vec::new();

    let known: &[(&str, &str, &[&str])] = &[
        ("Claude Code", "claude", &[]),
        ("OpenCode", "opencode", &[]),
        ("OMP", "omp", &[]),
        ("Aider", "aider", &[]),
        ("Gemini CLI", "gemini", &[]),
        ("Codex", "codex", &[]),
        ("Cursor", "cursor-agent", &[]),
        ("Cline", "cline", &[]),
        ("GitHub Copilot", "github-copilot-cli", &[]),
    ];

    for (label, binary, extra_args) in known {
        if binary_on_path(binary) {
            let mut argv: Vec<String> = vec![binary.to_string()];
            argv.extend(extra_args.iter().map(|s| s.to_string()));
            items.push(LauncherItem {
                label: label.to_string(),
                detail: binary.to_string(),
                kind: LauncherItemKind::Agent { argv },
            });
        }
    }

    items
}

/// Sync tab names from agent OSC titles for OMP and Claude Code.
pub fn sync_tab_agent_names(
    workspaces: &mut [crate::workspace::Workspace],
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
    terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
) -> bool {
    let synced: &[Agent] = &[Agent::Omp, Agent::Claude];
    let mut changed = false;
    for ws in workspaces {
        for tab in &mut ws.tabs {
            if tab.custom_name.is_some() {
                continue;
            }
            let mut new_title: Option<String> = None;
            for pane in tab.panes.values() {
                let Some(t) = terminals.get(&pane.attached_terminal_id) else {
                    continue;
                };
                let matches = t
                    .agent_name
                    .as_deref()
                    .is_some_and(|n| synced.iter().any(|a| crate::detect::agent_label(*a) == n));
                if !matches {
                    continue;
                }
                if let Some(rt) = terminal_runtimes.get(&pane.attached_terminal_id) {
                    let title = rt.agent_osc_title();
                    if !title.is_empty() && tab.synced_agent_name.as_deref() != Some(&title) {
                        new_title = Some(title);
                        break;
                    }
                }
            }
            if let Some(title) = new_title {
                tab.set_synced_agent_name(title);
                changed = true;
            }
        }
    }
    changed
}
