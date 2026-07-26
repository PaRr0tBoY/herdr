use std::path::PathBuf;

use crate::app::state::AppState;
use crate::app::{App, Mode};
use crate::layout::PaneId;
use crate::pane::PaneLaunchEnv;
use crate::popup_size::{resolve_popup_geometry, PopupSize};
use crate::terminal::{TerminalId, TerminalRuntime, TerminalState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PopupGeometry {
    pub width: Option<PopupSize>,
    pub height: Option<PopupSize>,
}

impl App {
    pub(crate) fn popup_runtime(&self) -> Option<&TerminalRuntime> {
        let terminal_id = &self.state.popup_pane.as_ref()?.terminal_id;
        self.terminal_runtimes.get(terminal_id)
    }

    pub(crate) fn close_popup_pane(&mut self) -> bool {
        let Some(popup) = self.state.popup_pane.take() else {
            return false;
        };
        self.state
            .direct_attach_resize_locks
            .remove(&popup.terminal_id);
        self.state.terminals.remove(&popup.terminal_id);
        if let Some(runtime) = self.terminal_runtimes.remove(&popup.terminal_id) {
            runtime.shutdown();
        }
        self.state.mode = if self.state.active.is_some() {
            Mode::Terminal
        } else {
            Mode::Navigate
        };
        self.render_dirty
            .store(true, std::sync::atomic::Ordering::Release);
        self.render_notify.notify_one();
        true
    }

    pub(crate) fn try_route_paste_to_popup(&mut self, text: &str) -> bool {
        if self.state.popup_pane.is_none() {
            return false;
        }
        let Some(runtime) = self.popup_runtime() else {
            self.close_popup_pane();
            return true;
        };
        let _ = runtime.try_send_paste(text.to_owned());
        true
    }

    pub(crate) fn spawn_popup_shell_command(
        &mut self,
        command: &str,
        cwd: Option<PathBuf>,
        extra_env: Vec<(String, String)>,
        geometry: PopupGeometry,
    ) -> std::io::Result<()> {
        self.spawn_popup_command(
            cwd,
            extra_env,
            geometry,
            |pane_id, rows, cols, cwd, launch_env, app| {
                TerminalRuntime::spawn_shell_command(
                    pane_id,
                    rows,
                    cols,
                    cwd,
                    command,
                    launch_env,
                    crate::pane::AgentDetection::Disabled,
                    app.state.pane_scrollback_limit_bytes,
                    app.state.host_terminal_theme,
                    app.event_tx.clone(),
                    app.render_notify.clone(),
                    app.render_dirty.clone(),
                )
                .map(|runtime| (runtime, None))
            },
        )
    }

    pub(crate) fn spawn_popup_argv_command(
        &mut self,
        argv: &[String],
        cwd: Option<PathBuf>,
        extra_env: Vec<(String, String)>,
        geometry: PopupGeometry,
    ) -> std::io::Result<()> {
        self.spawn_popup_command(
            cwd,
            extra_env,
            geometry,
            |pane_id, rows, cols, cwd, launch_env, app| {
                TerminalRuntime::spawn_argv_command(
                    pane_id,
                    rows,
                    cols,
                    cwd,
                    argv,
                    launch_env,
                    crate::pane::AgentDetection::Disabled,
                    app.state.pane_scrollback_limit_bytes,
                    app.state.host_terminal_theme,
                    app.event_tx.clone(),
                    app.render_notify.clone(),
                    app.render_dirty.clone(),
                )
                .map(|runtime| (runtime, Some(argv.to_vec())))
            },
        )
    }

    fn spawn_popup_command<F>(
        &mut self,
        cwd: Option<PathBuf>,
        extra_env: Vec<(String, String)>,
        geometry: PopupGeometry,
        spawn: F,
    ) -> std::io::Result<()>
    where
        F: FnOnce(
            PaneId,
            u16,
            u16,
            PathBuf,
            &PaneLaunchEnv,
            &mut App,
        ) -> std::io::Result<(TerminalRuntime, Option<Vec<String>>)>,
    {
        if self.state.popup_pane.is_some() {
            return Err(std::io::Error::other("popup already open"));
        }
        let Some(ws_idx) = self.state.active else {
            return Err(std::io::Error::other("no active workspace"));
        };
        let ws = self
            .state
            .workspaces
            .get(ws_idx)
            .ok_or_else(|| std::io::Error::other("active workspace disappeared"))?;
        let active_tab = ws
            .active_tab()
            .ok_or_else(|| std::io::Error::other("active tab disappeared"))?;
        let focused_pane = ws
            .focused_pane_id()
            .ok_or_else(|| std::io::Error::other("active tab has no focused pane"))?;
        let cwd = cwd.or_else(|| {
            active_tab.cwd_for_pane(focused_pane, &self.state.terminals, &self.terminal_runtimes)
        });
        let cwd = cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| "/".into()));
        let pane_id = PaneId::alloc();
        let terminal_id = TerminalId::alloc();
        let launch_env = PaneLaunchEnv::from_extra(extra_env).without_pane_identity();
        let terminal_area = if self.state.view.terminal_area.width >= 4
            && self.state.view.terminal_area.height >= 4
        {
            self.state.view.terminal_area
        } else {
            let (estimated_rows, estimated_cols) = self.state.estimate_pane_size();
            ratatui::layout::Rect::new(0, 0, estimated_cols, estimated_rows)
        };
        let Some(resolved_geometry) =
            resolve_popup_geometry(geometry.width, geometry.height, terminal_area)
        else {
            return Err(std::io::Error::other("terminal area too small for popup"));
        };
        let rows = resolved_geometry.inner.height;
        let cols = resolved_geometry.inner.width;
        let (runtime, launch_argv) = spawn(pane_id, rows, cols, cwd.clone(), &launch_env, self)?;
        let terminal = match launch_argv {
            Some(argv) => TerminalState::new(terminal_id.clone(), cwd).with_launch_argv(argv),
            None => TerminalState::new(terminal_id.clone(), cwd),
        };
        self.terminal_runtimes.insert(terminal_id.clone(), runtime);
        self.state.terminals.insert(terminal_id.clone(), terminal);
        self.state.popup_pane = Some(crate::app::state::PopupPaneState {
            pane_id,
            terminal_id,
            width: geometry.width,
            height: geometry.height,
        });
        self.state.mode = Mode::Terminal;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Note popup
// ---------------------------------------------------------------------------

impl App {
    /// Toggle the note popup for the active workspace.
    pub(crate) fn toggle_note_popup(&mut self) -> std::io::Result<()> {
        if self.state.note_popup_active {
            self.close_note_popup();
            return Ok(());
        }
        self.open_note_popup()
    }

    fn open_note_popup(&mut self) -> std::io::Result<()> {
        let ws_idx = self
            .state
            .active
            .ok_or_else(|| std::io::Error::other("no active workspace"))?;
        let ws_id = self.state.workspaces[ws_idx].id.clone();
        let note_path = crate::note::note_path(&ws_id);

        // Compute popup geometry.
        let terminal_area = self.state.view.terminal_area;
        let (w, h) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.note_popup_size)
            .unwrap_or_else(|| AppState::default_note_popup_size(terminal_area));
        let outer =
            AppState::compute_note_popup_rect(self.state.view.note_hit_area, terminal_area, w, h)
                .ok_or_else(|| std::io::Error::other("terminal too small for note popup"))?;

        // Populate the in-memory buffer from file on first open.
        let ws = &mut self.state.workspaces[ws_idx];
        if ws.note_buffer.is_empty() && note_path.exists() {
            ws.note_buffer = std::fs::read_to_string(&note_path).unwrap_or_default();
        }
        let content = std::mem::take(&mut ws.note_buffer);
        let mut textarea =
            ratatui_textarea::TextArea::new(content.lines().map(|l| l.to_string()).collect());
        textarea.set_style(
            ratatui::style::Style::default()
                .fg(self.state.palette.text)
                .bg(self.state.palette.panel_bg),
        );
        // Remove underline from cursor line.
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        // Restore cursor position from previous session.
        if let Some((row, col)) = self.state.note_cursor.take() {
            use ratatui_textarea::CursorMove;
            let max_row = textarea.lines().len().saturating_sub(1);
            let row = row.min(max_row);
            let col = col.min(textarea.lines().get(row).map(|l| l.chars().count()).unwrap_or(0));
            textarea.move_cursor(CursorMove::Jump(row as u16, col as u16));
        }
        self.state.note_textarea = Some(textarea);

        // Tag this popup as the note popup.
        self.state.note_popup_active = true;
        self.state.note_popup_workspace_id = Some(ws_id);
        self.state.note_popup_outer_rect = Some(outer);
        Ok(())
    }

    /// Close the note popup and reset note-specific state.
    pub(crate) fn close_note_popup(&mut self) {
        // Save text back to workspace buffer.
        if let Some(textarea) = self.state.note_textarea.take() {
            // Remember cursor position for next open.
            let cursor = textarea.cursor();
            self.state.note_cursor = Some((cursor.0, cursor.1));
            let content: String = textarea
                .lines()
                .iter()
                .map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if let Some(ws_id) = &self.state.note_popup_workspace_id.clone() {
                if let Some(ws) = self.state.workspaces.iter_mut().find(|w| &w.id == ws_id) {
                    ws.note_buffer = content;
                }
            }
        }
        self.state.note_popup_active = false;
        self.state.note_popup_workspace_id = None;
        self.state.note_popup_outer_rect = None;
    }
}

#[cfg(test)]
impl App {
    pub(crate) fn install_test_popup_runtime(
        &mut self,
        runtime: TerminalRuntime,
    ) -> (PaneId, TerminalId) {
        let pane_id = PaneId::alloc();
        let terminal_id = TerminalId::alloc();
        self.terminal_runtimes.insert(terminal_id.clone(), runtime);
        self.state.terminals.insert(
            terminal_id.clone(),
            TerminalState::new(terminal_id.clone(), PathBuf::from("/popup")),
        );
        self.state.popup_pane = Some(crate::app::state::PopupPaneState {
            pane_id,
            terminal_id: terminal_id.clone(),
            width: None,
            height: None,
        });
        (pane_id, terminal_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_popup() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![crate::workspace::Workspace::test_new("popup")];
        app.state.active = Some(0);
        app.state.selected = 0;
        let terminal_id = TerminalId::alloc();
        app.state.terminals.insert(
            terminal_id.clone(),
            TerminalState::new(terminal_id.clone(), PathBuf::from("/popup")),
        );
        app.state.popup_pane = Some(crate::app::state::PopupPaneState {
            pane_id: PaneId::alloc(),
            terminal_id,
            width: None,
            height: None,
        });
        app
    }

    #[test]
    fn close_popup_uses_terminal_mode_with_active_workspace() {
        let mut app = app_with_popup();
        app.state.mode = Mode::Navigate;

        assert!(app.close_popup_pane());

        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn close_popup_uses_navigate_mode_without_active_workspace() {
        let mut app = app_with_popup();
        app.state.workspaces.clear();
        app.state.active = None;
        app.state.mode = Mode::Navigate;

        assert!(app.close_popup_pane());

        assert_eq!(app.state.mode, Mode::Navigate);
    }

    #[test]
    fn close_popup_clears_direct_attach_resize_lock() {
        let mut app = app_with_popup();
        let terminal_id = app.state.popup_pane.as_ref().unwrap().terminal_id.clone();
        app.state
            .direct_attach_resize_locks
            .insert(terminal_id.clone());

        assert!(app.close_popup_pane());

        assert!(!app.state.direct_attach_resize_locks.contains(&terminal_id));
    }

    #[test]
    fn popup_survives_background_workspace_removal() {
        let mut app = app_with_popup();
        app.state.workspaces.clear();
        app.state.active = None;

        app.state.assert_invariants_for_test();

        assert!(app.state.popup_pane.is_some());
    }

    #[test]
    fn popup_close_api_closes_only_active_popup() {
        let mut app = app_with_popup();
        let close = || crate::api::schema::Request {
            id: "close-popup".into(),
            method: crate::api::schema::Method::PopupClose(
                crate::api::schema::EmptyParams::default(),
            ),
        };

        let response = app.handle_api_request(close());
        let response: crate::api::schema::SuccessResponse =
            serde_json::from_str(&response).unwrap();
        assert_eq!(response.result, crate::api::schema::ResponseResult::Ok {});

        let response = app.handle_api_request(close());
        let response: crate::api::schema::ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(response.error.code, "popup_not_open");
    }
}
