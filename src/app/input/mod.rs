//! Input handling — translates crossterm key/mouse events into state mutations.

use bytes::Bytes;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use tracing::warn;

use crate::app::PaneClickState;
use crate::input::TerminalKey;
#[cfg(test)]
use ratatui::layout::Direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrollbarClickTarget {
    Thumb { grab_row_offset: u16 },
    Track { offset_from_bottom: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
enum WheelRouting {
    HostScroll,
    MouseReport,
    AlternateScroll,
}

const WORKSPACE_DRAG_THRESHOLD: u16 = 1;
const TAB_DRAG_THRESHOLD: u16 = 1;

fn modified_url_click_modifier() -> KeyModifiers {
    KeyModifiers::CONTROL
}

#[cfg(test)]
#[test]
fn modified_url_click_modifier_matches_terminal_mouse_reporting() {
    assert_eq!(modified_url_click_modifier(), KeyModifiers::CONTROL);
}

mod clipboard;
mod copy_mode;
mod modal;
mod mouse;
mod navigate;
mod overlays;
mod selection;
mod settings;
mod sidebar;
mod terminal;

pub(crate) use self::{
    modal::{
        handle_global_menu_key, handle_keybind_help_key, handle_navigator_key,
        insert_keybind_help_query_text, insert_navigator_search_text, insert_rename_input_text,
        open_new_workspace_dialog,
    },
    navigate::{
        terminal_direct_indexed_navigation_action, terminal_direct_non_indexed_navigation_action,
    },
    settings::open_settings_at,
};
use self::{
    modal::{
        modal_action_from_key, ModalAction, ONBOARDING_WELCOME_ACTIONS, RELEASE_NOTES_ACTIONS,
    },
    mouse::MouseAction,
    settings::SettingsAction,
};
use super::state::{AppState, Mode};
use super::App;

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

impl App {
    pub(super) async fn handle_key(
        &mut self,
        key: TerminalKey,
    ) -> Option<super::TerminalInputTarget> {
        // Note popup captures all keyboard input.
        if self.state.note_popup_active {
            self.handle_note_popup_key(key);
            return None;
        }
        if self.state.popup_pane.is_some() {
            return self.handle_terminal_key(key).await;
        }
        let key_event = key.as_key_event();
        if modal_paste_target_active(&self.state) && is_modal_paste_shortcut(&key_event) {
            if let Some(text) = crate::platform::read_clipboard_text() {
                self.paste_into_active_text_input(&text);
            }
            return None;
        }

        match self.state.mode {
            Mode::Terminal => return self.handle_terminal_key(key).await,
            Mode::Prefix => self.handle_prefix_key(key),
            Mode::Navigate => self.handle_navigate_key(key),
            Mode::Copy => self.handle_copy_mode_key(key),
            Mode::NewTabType => self.handle_new_tab_type_key(key),
            _ => match self.state.mode {
                Mode::Onboarding => self.handle_onboarding_key(key_event),
                Mode::ReleaseNotes => self.handle_release_notes_key(key_event),
                Mode::ProductAnnouncement => self.handle_product_announcement_key(key_event),
                Mode::Prefix | Mode::Navigate | Mode::Copy => unreachable!(),
                Mode::RenameWorkspace | Mode::RenameTab | Mode::RenamePane => {
                    self.handle_rename_key_via_api(key_event)
                }
                Mode::NewLinkedWorktree => self.handle_worktree_create_key(key_event),
                Mode::OpenExistingWorktree => self.handle_worktree_open_key(key_event),
                Mode::ConfirmRemoveWorktree => self.handle_worktree_remove_key(key_event),
                Mode::Resize => self.handle_resize_key_via_api(key),
                Mode::ConfirmClose => self.handle_confirm_close_key_via_api(key_event),
                Mode::ContextMenu => {
                    self.handle_context_menu_key_via_api(key_event);
                }
                Mode::Settings => self.handle_settings_key(key_event),
                Mode::GlobalMenu => handle_global_menu_key(&mut self.state, key_event),
                Mode::KeybindHelp => handle_keybind_help_key(&mut self.state, key),
                Mode::Navigator => {
                    handle_navigator_key(&mut self.state, &self.terminal_runtimes, key_event)
                }
                Mode::NewTabType => unreachable!(),
                Mode::Terminal => unreachable!(),
            },
        }
        None
    }

    pub(super) async fn handle_paste(&mut self, text: String) {
        tracing::debug!(
            note_popup_active = self.state.note_popup_active,
            len = text.len(),
            "handle_paste"
        );
        if self.state.note_popup_active {
            if let Some(textarea) = &mut self.state.note_textarea {
                textarea.insert_str(text);
            }
            return;
        }
        if self.state.popup_pane.is_some() {
            if let Some(runtime) = self.popup_runtime() {
                let _ = runtime.send_paste(text).await;
            } else {
                self.close_popup_pane();
            }
            return;
        }
        if self.state.mode != Mode::Terminal {
            self.paste_into_active_text_input(&text);
            return;
        }

        if let Some(ws_idx) = self.state.active {
            if let Some(rt) = self
                .state
                .focused_runtime_in_workspace(&self.terminal_runtimes, ws_idx)
            {
                let _ = rt.send_paste(text).await;
            }
        }
    }

    pub(crate) fn paste_into_active_text_input(&mut self, text: &str) -> bool {
        match self.state.mode {
            Mode::RenameWorkspace | Mode::RenameTab | Mode::RenamePane => {
                insert_rename_input_text(&mut self.state, text);
                true
            }
            Mode::NewLinkedWorktree => {
                self.insert_worktree_create_text(text);
                true
            }
            Mode::OpenExistingWorktree => {
                if !self
                    .state
                    .worktree_open
                    .as_ref()
                    .is_some_and(|open| open.search_focused)
                {
                    return false;
                }
                self.insert_worktree_open_search_text(text);
                true
            }
            Mode::Navigator => {
                if !self.state.navigator.search_focused {
                    return false;
                }
                insert_navigator_search_text(&mut self.state, &self.terminal_runtimes, text);
                true
            }
            Mode::KeybindHelp => {
                if !self.state.keybind_help.search_focused {
                    return false;
                }
                insert_keybind_help_query_text(&mut self.state, text);
                true
            }
            Mode::Copy => {
                let Some(prompt) = self
                    .state
                    .copy_mode
                    .as_mut()
                    .and_then(|copy_mode| copy_mode.search.prompt.as_mut())
                else {
                    return false;
                };
                prompt
                    .query
                    .extend(text.chars().filter(|ch| !ch.is_control()));
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_onboarding_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Right | KeyCode::Char('l') => self.open_settings_from_onboarding(),
            _ => {
                if let Some(ModalAction::Continue) =
                    modal_action_from_key(&key, ONBOARDING_WELCOME_ACTIONS)
                {
                    self.open_settings_from_onboarding();
                }
            }
        }
    }

    pub(crate) fn handle_release_notes_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.scroll_release_notes(-1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_release_notes(1),
            KeyCode::PageUp => self.scroll_release_notes(-8),
            KeyCode::PageDown => self.scroll_release_notes(8),
            KeyCode::Home => {
                if let Some(notes) = &mut self.state.release_notes {
                    notes.scroll = 0;
                }
            }
            KeyCode::End => {
                let max_scroll = self.state.release_notes_max_scroll();
                if let Some(notes) = &mut self.state.release_notes {
                    notes.scroll = max_scroll;
                }
            }
            _ => {
                if let Some(ModalAction::Close) = modal_action_from_key(&key, RELEASE_NOTES_ACTIONS)
                {
                    self.dismiss_release_notes();
                }
            }
        }
    }

    pub(crate) fn handle_product_announcement_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.scroll_product_announcement(-1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_product_announcement(1),
            KeyCode::PageUp => self.scroll_product_announcement(-8),
            KeyCode::PageDown => self.scroll_product_announcement(8),
            KeyCode::Home => {
                if let Some(announcement) = &mut self.state.product_announcement {
                    announcement.scroll = 0;
                }
            }
            KeyCode::End => {
                let max_scroll = self.state.product_announcement_max_scroll();
                if let Some(announcement) = &mut self.state.product_announcement {
                    announcement.scroll = max_scroll;
                }
            }
            _ => {
                if let Some(ModalAction::Close) = modal_action_from_key(&key, RELEASE_NOTES_ACTIONS)
                {
                    self.dismiss_product_announcement();
                }
            }
        }
    }

    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent) {
        self.handle_mouse_from_input_source(super::LOCAL_INPUT_SOURCE, mouse);
    }

    pub(super) fn handle_mouse_from_input_source(
        &mut self,
        source_id: super::InputSourceId,
        mouse: MouseEvent,
    ) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.pending_url_click_sources.remove(&source_id);
            }
            MouseEventKind::Drag(MouseButton::Left)
                if self.pending_url_click_sources.contains(&source_id) =>
            {
                return;
            }
            MouseEventKind::Up(MouseButton::Left)
                if self.pending_url_click_sources.remove(&source_id) =>
            {
                return;
            }
            _ => {}
        }

        // Note popup overlay: click outside → close, resize drag → handle.
        if self.state.note_popup_active {
            self.handle_note_popup_mouse(mouse);
            return;
        }
        if self.state.popup_pane.is_some() {
            self.handle_popup_mouse(mouse);
            return;
        }
        if self.handle_overlay_mouse(mouse) {
            return;
        }

        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && self.state.on_sidebar_divider(mouse.column, mouse.row)
        {
            let now = std::time::Instant::now();
            let is_double_click = self
                .last_sidebar_divider_click
                .is_some_and(|last| now.duration_since(last) <= super::SIDEBAR_DOUBLE_CLICK_WINDOW);
            self.last_sidebar_divider_click = Some(now);

            if is_double_click {
                self.state.sidebar_width = self.state.default_sidebar_width;
                self.state.sidebar_width_source =
                    crate::app::state::SidebarWidthSource::ConfigDefault;
                self.state.sidebar_width_auto = false;
                self.state.mark_session_dirty();
                self.state.drag = None;
                return;
            }
        }

        if self.handle_modified_url_click(source_id, mouse) {
            return;
        }

        let handled_pane_double_click = self.handle_pane_double_click(mouse);
        if !handled_pane_double_click {
            self.focus_pane_before_mouse_press(mouse);
        }

        let previous_agent_panel_sort = self.state.agent_panel_sort;
        let previous_settings_section = self.state.settings.section;
        if !handled_pane_double_click {
            if let Some(action) = self.state.handle_mouse(&mut self.terminal_runtimes, mouse) {
                match action {
                    MouseAction::NewWorkspace => {
                        self.begin_tui_workspace_create("tui.mouse.workspace.create")
                    }
                    MouseAction::Settings(action) => match action {
                        SettingsAction::SaveTheme(name) => self.save_theme(&name),
                        SettingsAction::SaveSound(enabled) => self.save_sound(enabled),
                        SettingsAction::SaveToastDelivery(delivery) => {
                            self.save_toast_delivery(delivery)
                        }
                        SettingsAction::SaveAgentBorderLabels(enabled) => {
                            self.save_agent_border_labels(enabled)
                        }
                        SettingsAction::SavePaneHistory(enabled) => {
                            self.save_pane_history_persistence(enabled)
                        }
                        SettingsAction::SaveSwitchAsciiInputSourceInPrefix(enabled) => {
                            self.save_switch_ascii_input_source_in_prefix(enabled)
                        }
                        SettingsAction::InstallRecommendedIntegrations => {
                            self.install_recommended_integrations()
                        }
                    },
                    MouseAction::FocusWorkspace { ws_idx } => {
                        self.focus_workspace_idx_via_api(ws_idx)
                    }
                    MouseAction::FocusTab { tab_idx } => self.focus_tab_idx_via_api(tab_idx),
                    MouseAction::FocusPane { ws_idx, pane_id } => {
                        self.focus_pane_internal_via_api(ws_idx, pane_id)
                    }
                    MouseAction::FocusToastTarget => self.focus_toast_target_via_api(),
                    MouseAction::MoveWorkspace {
                        source_ws_idx,
                        insert_idx,
                    } => self.move_workspace_via_api(source_ws_idx, insert_idx),
                    MouseAction::MoveWorkspaceBlock { params } => {
                        self.move_workspace_block_via_api(params)
                    }
                    MouseAction::MoveTab {
                        ws_idx,
                        source_tab_idx,
                        insert_idx,
                    } => self.move_tab_via_api(ws_idx, source_tab_idx, insert_idx),
                    MouseAction::SetSplitRatio { path, ratio } => {
                        self.set_split_ratio_via_api(path, ratio)
                    }
                    MouseAction::RenameModal(action) => {
                        self.apply_rename_mouse_action_via_api(action)
                    }
                    MouseAction::ConfirmCloseAccept => self.confirm_close_accept_via_api(),
                    MouseAction::ContextMenu { menu, idx } => {
                        self.apply_context_menu_action_via_api(menu, idx)
                    }
                    MouseAction::ToggleNotePopup => {
                        if let Err(err) = self.toggle_note_popup() {
                            tracing::warn!(err = %err, "failed to toggle note popup");
                        }
                    }
                }
            }
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                && self
                    .state
                    .selection
                    .as_ref()
                    .is_none_or(crate::selection::Selection::is_in_progress)
            {
                self.selection_highlight_clear_deadline = None;
            }
        }
        if previous_settings_section != crate::app::state::SettingsSection::Integrations
            && self.state.settings.section == crate::app::state::SettingsSection::Integrations
        {
            self.refresh_integration_recommendations();
        }
        if self.state.agent_panel_sort != previous_agent_panel_sort {
            self.save_agent_panel_sort(self.state.agent_panel_sort);
        }

        self.dispatch_pending_clipboard_write();

        // Sync autoscroll deadline with state (mouse handler may have
        // set or cleared selection_autoscroll during handle_mouse).
        if self.state.selection_autoscroll.is_none() {
            self.selection_autoscroll_deadline = None;
        } else if self.selection_autoscroll_deadline.is_none() {
            self.selection_autoscroll_deadline =
                Some(std::time::Instant::now() + super::SELECTION_AUTOSCROLL_INTERVAL);
        }
    }

    fn handle_popup_mouse(&mut self, mouse: MouseEvent) {
        let Some((_outer, inner)) =
            crate::ui::popup_pane_rects(&self.state, self.state.view.terminal_area)
        else {
            return;
        };
        if mouse.column < inner.x
            || mouse.column >= inner.x.saturating_add(inner.width)
            || mouse.row < inner.y
            || mouse.row >= inner.y.saturating_add(inner.height)
        {
            return;
        }
        let Some(rt) = self.popup_runtime() else {
            self.close_popup_pane();
            return;
        };
        let column = mouse.column.saturating_sub(inner.x);
        let row = mouse.row.saturating_sub(inner.y);
        let bytes = match mouse.kind {
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => match rt.wheel_routing() {
                Some(crate::pane::WheelRouting::MouseReport) => {
                    rt.encode_mouse_wheel(mouse.kind, column, row, mouse.modifiers)
                }
                Some(crate::pane::WheelRouting::AlternateScroll) => {
                    rt.encode_alternate_scroll(mouse.kind)
                }
                Some(crate::pane::WheelRouting::HostScroll) | None => {
                    let lines_per_notch = self.state.mouse_scroll_lines;
                    match mouse.kind {
                        MouseEventKind::ScrollUp => rt.scroll_up(lines_per_notch),
                        MouseEventKind::ScrollDown => rt.scroll_down(lines_per_notch),
                        _ => {}
                    }
                    return;
                }
            },
            MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::Drag(_) => {
                rt.encode_mouse_button(mouse.kind, column, row, mouse.modifiers)
            }
            MouseEventKind::Moved => {
                rt.encode_mouse_motion(mouse.kind, column, row, mouse.modifiers)
            }
        };
        let Some(bytes) = bytes else {
            return;
        };
        if !matches!(mouse.kind, MouseEventKind::Moved) {
            rt.scroll_reset();
        }
        if let Err(err) = rt.try_send_bytes(Bytes::from(bytes)) {
            warn!(err = %err, kind = ?mouse.kind, "failed to forward popup mouse event");
        }
    }

    /// Handle mouse events for the note popup overlay.
    pub(crate) fn handle_note_popup_mouse(&mut self, mouse: MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        use ratatui::layout::Rect;

        let Some(outer) = self.state.note_popup_outer_rect else {
            return;
        };

        let corner = Rect::new(
            outer.x,
            outer.y + outer.height.saturating_sub(1),
            2.min(outer.width),
            1.min(outer.height),
        );
        // Text area: inner rect minus 1px border on all sides.
        let text_area = Rect::new(
            outer.x.saturating_add(1),
            outer.y.saturating_add(1),
            outer.width.saturating_sub(2).max(1),
            outer.height.saturating_sub(2).max(1),
        );

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if mouse.column >= corner.x
                    && mouse.column < corner.x + corner.width
                    && mouse.row >= corner.y
                    && mouse.row < corner.y + corner.height
                {
                    self.state.drag = Some(crate::app::state::DragState {
                        target: crate::app::state::DragTarget::NotePopupResize {
                            start_rect: outer,
                            start_col: mouse.column,
                            start_row: mouse.row,
                        },
                    });
                    return;
                }
                if mouse.column < outer.x
                    || mouse.column >= outer.x + outer.width
                    || mouse.row < outer.y
                    || mouse.row >= outer.y + outer.height
                {
                    self.close_note_popup();
                    return;
                }
                // Click inside text area: position cursor, cancel selection.
                if let Some(textarea) = &mut self.state.note_textarea {
                    if mouse.column >= text_area.x
                        && mouse.column < text_area.x + text_area.width
                        && mouse.row >= text_area.y
                        && mouse.row < text_area.y + text_area.height
                    {
                        // Cancel any previous selection and place cursor at click point.
                        textarea.cancel_selection();
                        let target_sr = mouse.row.saturating_sub(text_area.y);
                        let target_sc = mouse.column.saturating_sub(text_area.x);
                        Self::move_textarea_cursor_to_screen(textarea, target_sr, target_sc);
                        self.state.drag = Some(crate::app::state::DragState {
                            target: crate::app::state::DragTarget::NoteTextSelect,
                        });
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                match &self.state.drag {
                    Some(crate::app::state::DragState {
                        target:
                            crate::app::state::DragTarget::NotePopupResize {
                                start_rect,
                                start_col,
                                start_row,
                            },
                    }) => {
                        self.state.handle_note_popup_resize_drag(
                            mouse.column,
                            mouse.row,
                            *start_rect,
                            *start_col,
                            *start_row,
                        );
                    }
                    Some(crate::app::state::DragState {
                        target: crate::app::state::DragTarget::NoteTextSelect,
                    }) => {
                        if let Some(textarea) = &mut self.state.note_textarea {
                            if !textarea.is_selecting() {
                                textarea.start_selection();
                            }
                            let target_sr = mouse.row.saturating_sub(text_area.y);
                            let target_sc = mouse.column.saturating_sub(text_area.x);
                            // move_cursor preserves selection when active
                            // (internally passes shift=true when selection_start.is_some()).
                            Self::move_textarea_cursor_to_screen(textarea, target_sr, target_sc);
                        }
                    }
                    _ => {}
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // If text was selected, copy to clipboard.
                if matches!(
                    self.state.drag.as_ref().map(|d| &d.target),
                    Some(crate::app::state::DragTarget::NoteTextSelect),
                ) {
                    if let Some(textarea) = &mut self.state.note_textarea {
                        if textarea.is_selecting() {
                            textarea.copy();
                            let yanked = textarea.yank_text();
                            textarea.cancel_selection();
                            if !yanked.is_empty() {
                                self.state.request_clipboard_write = Some(yanked.into_bytes());
                                self.state.copy_feedback = Some(crate::app::state::CopyFeedback {
                                    message: "copied to clipboard".to_string(),
                                });
                                self.copy_feedback_deadline =
                                    Some(std::time::Instant::now() + super::COPY_FEEDBACK_DURATION);
                                self.dispatch_pending_clipboard_write();
                            }
                        }
                    }
                }
                self.state.drag = None;
            }
            _ => {}
        }
    }

    /// Move textarea cursor to a screen-relative position (row, col)
    /// within the text area viewport. Translates screen coordinates to
    /// data coordinates via scroll offset from the current cursor,
    /// then uses `CursorMove::Jump` for instant, precise placement.
    /// When textarea selection is active, `move_cursor` internally passes
    /// `shift=true`, so selection extends from origin to target.
    fn move_textarea_cursor_to_screen(
        textarea: &mut ratatui_textarea::TextArea,
        screen_row: u16,
        screen_col: u16,
    ) {
        use ratatui_textarea::CursorMove;
        use unicode_width::UnicodeWidthChar;

        // Phase 1: Row movement via step-based Down/Up.
        // These correctly handle WrapMode::Word wrapping.
        let scr = textarea.screen_cursor();
        let d_row = screen_row as i32 - scr.row as i32;
        let row_mover = if d_row > 0 {
            CursorMove::Down
        } else {
            CursorMove::Up
        };
        for _ in 0..(d_row.unsigned_abs() as usize).min(500) {
            textarea.move_cursor(row_mover);
        }

        // Phase 2: Column via display-width-aware mapping.
        // Forward/Back move by grapheme clusters, not screen columns,
        // so they misalign with CJK/wide characters. Instead we compute
        // the target data column using the current cursor as an anchor:
        // cursor() and screen_cursor() form a stable (data, screen) pair
        // from the last render, breaking the scroll-offset feedback loop.
        let scr = textarea.screen_cursor();
        let data = textarea.cursor();
        let line = &textarea.lines()[data.0];

        // Display width from line start to current cursor position.
        let cursor_disp_col: usize = line
            .chars()
            .take(data.1)
            .map(|c| c.width().unwrap_or(0))
            .sum();

        // Target display width from line start.
        // Works with WrapMode::Word because cursor_disp_col - scr.col
        // gives the display offset of the current wrapped segment start.
        let d_col = screen_col as i32 - scr.col as i32;
        let target_disp_col = (cursor_disp_col as i32 + d_col).max(0) as usize;

        // Walk characters from line start to find the data column
        // whose cumulative display width reaches target_disp_col.
        let mut acc = 0usize;
        let mut target_col = line.chars().count(); // default: end of line
        for (i, c) in line.chars().enumerate() {
            let w = c.width().unwrap_or(0);
            if acc + w > target_disp_col {
                // Click landed inside a wide character (e.g. CJK) —
                // snap to nearest character boundary.
                target_col = if target_disp_col - acc >= w.div_ceil(2) {
                    i + 1
                } else {
                    i
                };
                break;
            }
            acc += w;
            target_col = i + 1;
        }

        textarea.move_cursor(CursorMove::Jump(data.0 as u16, target_col as u16));
    }

    /// Route keyboard events to the note popup TextArea.
    fn handle_note_popup_key(&mut self, key: TerminalKey) {
        let Some(textarea) = &mut self.state.note_textarea else {
            return;
        };
        let key_event = key.as_key_event();
        // Ctrl+S: save to file.
        if key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
            && key.code == crossterm::event::KeyCode::Char('s')
        {
            self.state.save_note_to_file();
            return;
        }
        // Ctrl+V: paste from clipboard.
        if key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
            && key.code == crossterm::event::KeyCode::Char('v')
        {
            if let Some(text) = crate::platform::read_clipboard_text() {
                textarea.insert_str(&text);
                self.state.save_note_to_file();
            }
            return;
        }
        // Ctrl+Z: undo.
        if key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
            && key.code == crossterm::event::KeyCode::Char('z')
        {
            textarea.undo();
            return;
        }
        // Ctrl+Y: redo.
        if key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
            && key.code == crossterm::event::KeyCode::Char('y')
        {
            textarea.redo();
            return;
        }
        if key.code == crossterm::event::KeyCode::Esc {
            // Esc is a no-op in the note popup.
            // Closing via Esc conflicts with bracketed paste sequences
            // (\x1b[200~…) which arrive on Windows as individual key events
            // with Esc as the first character.
            // Close the popup by clicking outside or toggling the note button.
            return;
        }
        let input: ratatui_textarea::Input = key_event.into();
        tracing::debug!(?key.code, ?key.modifiers, "note popup key");
        textarea.input(input);
        // Autosave on every edit so notes survive crashes / taskkill / shutdown.
        self.state.save_note_to_file();
    }

    fn focus_pane_before_mouse_press(&mut self, mouse: MouseEvent) {
        if !matches!(self.state.mode, Mode::Terminal | Mode::Resize)
            || !matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left | MouseButton::Middle)
            )
        {
            return;
        }

        let Some(pane_id) = self
            .state
            .pane_at(mouse.column, mouse.row)
            .map(|info| info.id)
        else {
            return;
        };
        let Some(ws_idx) = self.state.active else {
            return;
        };

        // Focus through the runtime API before an application can consume its press.
        self.focus_pane_internal_via_api(ws_idx, pane_id);
    }

    fn handle_modified_url_click(
        &mut self,
        source_id: super::InputSourceId,
        mouse: MouseEvent,
    ) -> bool {
        if self.state.mode != Mode::Terminal
            || !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            || !mouse.modifiers.contains(modified_url_click_modifier())
        {
            return false;
        }

        let Some(info) = self.state.pane_at(mouse.column, mouse.row).cloned() else {
            return false;
        };
        let viewport_row = mouse.row.saturating_sub(info.inner_rect.y);
        let col = mouse.column.saturating_sub(info.inner_rect.x);
        let Some(url) =
            self.state
                .url_at_pane_cell(&self.terminal_runtimes, info.id, viewport_row, col)
        else {
            return false;
        };

        self.last_pane_click = None;
        self.pending_url_click_sources.insert(source_id);
        match self.invoke_plugin_link_handler_for_url(&url, info.id) {
            Ok(true) => return true,
            Ok(false) => {}
            Err(err) => {
                tracing::warn!(err = %err, url = %url, "failed to invoke plugin link handler");
            }
        }
        if let Err(err) = crate::platform::open_url(&url) {
            tracing::warn!(err = %err, url = %url, "failed to open pane URL");
        }
        true
    }

    fn handle_pane_double_click(&mut self, mouse: MouseEvent) -> bool {
        // A pane press stops being a double-click candidate once it becomes
        // a drag or completes as a real text selection.
        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                self.last_pane_click = None;
                return false;
            }
            MouseEventKind::Up(MouseButton::Left)
                if self
                    .state
                    .selection
                    .as_ref()
                    .is_some_and(|selection| selection.is_visible()) =>
            {
                self.last_pane_click = None;
                return false;
            }
            _ => {}
        }

        // Only terminal-pane left-clicks can start this gesture; other clicks
        // should keep their existing mouse behavior and clear stale candidates.
        let Some(click) = self.pane_click_candidate(mouse) else {
            return false;
        };

        // Require the second click to land near the first click in the same pane
        // and within the double-click window so adjacent interactions do not select a word.
        if !self.take_pane_double_click(click) {
            return false;
        }

        self.select_double_clicked_word(click)
    }

    fn pane_click_candidate(&mut self, mouse: MouseEvent) -> Option<PaneClickState> {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return None;
        }

        if !mouse.modifiers.is_empty() {
            self.last_pane_click = None;
            return None;
        }

        if self.state.mode != Mode::Terminal {
            self.last_pane_click = None;
            return None;
        }

        let Some(info) = self.state.pane_at(mouse.column, mouse.row).cloned() else {
            self.last_pane_click = None;
            return None;
        };

        Some(PaneClickState {
            pane_id: info.id,
            viewport_row: mouse.row - info.inner_rect.y,
            col: mouse.column - info.inner_rect.x,
            at: std::time::Instant::now(),
        })
    }

    fn take_pane_double_click(&mut self, click: PaneClickState) -> bool {
        if !self
            .last_pane_click
            .is_some_and(|last| last.is_double_click_for(click))
        {
            self.last_pane_click = Some(click);
            return false;
        }

        self.last_pane_click = None;
        true
    }

    fn select_double_clicked_word(&mut self, click: PaneClickState) -> bool {
        let selected = self.state.select_word_at_pane_cell(
            &self.terminal_runtimes,
            click.pane_id,
            click.viewport_row,
            click.col,
        );
        if selected {
            self.selection_highlight_clear_deadline = self
                .state
                .copy_on_select
                .then(|| std::time::Instant::now() + super::PANE_COPY_HIGHLIGHT_DURATION);
        }
        selected
    }
}

pub(crate) fn is_modal_paste_shortcut(key: &KeyEvent) -> bool {
    if !matches!(key.code, KeyCode::Char('v' | 'V')) {
        return false;
    }

    #[cfg(target_os = "macos")]
    {
        key.modifiers.contains(KeyModifiers::SUPER) || key.modifiers.contains(KeyModifiers::CONTROL)
    }

    #[cfg(not(target_os = "macos"))]
    {
        key.modifiers.contains(KeyModifiers::CONTROL)
    }
}

pub(crate) fn modal_paste_target_active(state: &AppState) -> bool {
    match state.mode {
        Mode::RenameWorkspace | Mode::RenameTab | Mode::RenamePane | Mode::NewLinkedWorktree => {
            true
        }
        Mode::OpenExistingWorktree => state
            .worktree_open
            .as_ref()
            .is_some_and(|open| open.search_focused),
        Mode::Navigator => state.navigator.search_focused,
        Mode::KeybindHelp => state.keybind_help.search_focused,
        Mode::Copy => state
            .copy_mode
            .as_ref()
            .is_some_and(|copy_mode| copy_mode.search.prompt.is_some()),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Mouse handling
// ---------------------------------------------------------------------------

// Note: split_pane needs runtime (event_tx for PTY spawn), so it lives on App
impl AppState {
    #[cfg(test)]
    pub(crate) fn split_pane(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        direction: Direction,
    ) {
        // Actual PTY spawning happens in Workspace::split_focused
        // which needs events channel — this is called from navigate_key
        // where we don't have async context, so the workspace handles it
        let (rows, cols) = self.estimate_pane_size();
        let new_rows = (rows / 2).max(4);
        let new_cols = (cols / 2).max(10);

        let follow_cwd = self
            .active
            .and_then(|i| self.workspaces.get(i))
            .and_then(|ws| {
                let tab = ws.active_tab()?;
                let terminal_id = tab.terminal_id(tab.layout.focused())?;
                super::creation::launch_cwd_for_terminal(
                    terminal_id,
                    &self.terminals,
                    terminal_runtimes,
                )
            });
        let cwd = Some(super::creation::resolve_new_terminal_cwd(
            &self.new_terminal_cwd,
            follow_cwd,
        ));

        let previous_focus = self.current_pane_focus_target();
        if let Some(ws_idx) = self.active {
            let Some(ws) = self.workspaces.get_mut(ws_idx) else {
                return;
            };
            if let Ok(new_pane) = ws.split_focused(
                direction,
                new_rows,
                new_cols,
                cwd,
                self.pane_scrollback_limit_bytes,
                self.host_terminal_theme,
                crate::pane::PaneShellConfig::new(&self.default_shell, self.shell_mode),
                Vec::new(),
            ) {
                let new_id = new_pane.pane_id;
                terminal_runtimes.insert(new_pane.terminal.id.clone(), new_pane.runtime);
                self.remove_alias_shadowed_by_new_pane(new_id);
                self.terminals
                    .insert(new_pane.terminal.id.clone(), new_pane.terminal);
                self.record_pane_focus_change(previous_focus, ws_idx, new_id);
                self.mark_session_dirty();
                self.mode = Mode::Terminal;
            }
        }
    }
}

#[cfg(test)]
fn state_with_workspaces(names: &[&str]) -> AppState {
    let mut state = AppState::test_new();
    state.workspaces = names
        .iter()
        .map(|name| crate::workspace::Workspace::test_new(name))
        .collect();
    if !state.workspaces.is_empty() {
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Navigate;
    }
    state
}

#[cfg(test)]
fn app_for_mouse_test() -> App {
    let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(
        &crate::config::Config::default(),
        true,
        None,
        api_rx,
        crate::api::EventHub::default(),
    );
    app.state.mode = Mode::Terminal;
    app.state.update_available = None;
    app.state.latest_release_notes_available = false;
    app.state.view.sidebar_rect = ratatui::layout::Rect::new(0, 0, 26, 20);
    app.state.view.terminal_area = ratatui::layout::Rect::new(26, 0, 80, 20);
    app
}

#[cfg(test)]
fn mouse(
    kind: crossterm::event::MouseEventKind,
    col: u16,
    row: u16,
) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind,
        column: col,
        row,
        modifiers: crossterm::event::KeyModifiers::empty(),
    }
}

#[cfg(test)]
fn numbered_lines_bytes(count: usize) -> Vec<u8> {
    (0..count)
        .map(|i| format!("{i:06}\r\n"))
        .collect::<String>()
        .into_bytes()
}

#[cfg(test)]
fn capture_snapshot(state: &AppState) -> crate::persist::SessionSnapshot {
    let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
    crate::persist::capture(
        &state.workspaces,
        &state.terminals,
        &terminal_runtimes,
        state.active,
        state.selected,
        state.sidebar_width,
        state.sidebar_section_split,
        state.collapsed_space_keys.clone(),
    )
}

#[cfg(test)]
fn root_layout_ratio(snapshot: &crate::persist::SessionSnapshot) -> Option<f32> {
    match &snapshot.workspaces.first()?.tabs.first()?.layout {
        crate::persist::LayoutSnapshot::Split { ratio, .. } => Some(*ratio),
        crate::persist::LayoutSnapshot::Pane(_) => None,
    }
}

#[cfg(test)]
fn unique_temp_path(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("hive-{name}-{}-{nanos}", std::process::id()))
}

#[cfg(test)]
#[cfg(unix)]
fn wait_for_file(path: &std::path::Path) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Ok(content) = std::fs::read_to_string(path) {
            if !content.is_empty() {
                return content;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("timed out waiting for {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        )
    }

    #[tokio::test]
    async fn paste_routes_to_rename_modal_input() {
        let mut app = test_app();
        app.state.workspaces = vec![crate::workspace::Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::RenameTab;
        app.state.name_input = "2".into();
        app.state.name_input_replace_on_type = true;

        app.handle_paste("feature/logs".into()).await;

        assert_eq!(app.state.name_input, "feature/logs");
        assert!(!app.state.name_input_replace_on_type);
    }

    #[tokio::test]
    async fn paste_routes_to_keybind_help_query_only_when_searching() {
        let mut app = test_app();
        app.state.mode = Mode::KeybindHelp;
        app.handle_paste("ignored".into()).await;
        assert!(app.state.keybind_help.query.is_empty());

        app.state.keybind_help.search_focused = true;
        app.state.keybind_help.scroll = 3;
        app.handle_paste("work\nspace".into()).await;

        assert_eq!(app.state.keybind_help.query, "workspace");
        assert_eq!(app.state.keybind_help.scroll, 0);
    }

    #[tokio::test]
    async fn paste_routes_to_new_linked_worktree_input() {
        let mut app = test_app();
        app.state.mode = Mode::NewLinkedWorktree;
        app.state.name_input = "generated-branch".into();
        app.state.name_input_replace_on_type = true;
        app.state.worktree_create = Some(crate::app::state::WorktreeCreateState {
            source_workspace_id: "source".into(),
            source_checkout_path: "/repo/herdr".into(),
            source_existing_membership: None,
            source_repo_root: "/repo/herdr".into(),
            repo_key: "repo-key".into(),
            repo_name: "herdr".into(),
            branch: "generated-branch".into(),
            checkout_path: "/repo/herdr-generated-branch".into(),
            error: None,
            creating: false,
        });

        app.handle_paste("feature/linear-302".into()).await;

        assert_eq!(app.state.name_input, "feature/linear-302");
        assert_eq!(
            app.state
                .worktree_create
                .as_ref()
                .map(|create| create.branch.as_str()),
            Some("feature/linear-302")
        );
    }

    #[test]
    fn modal_paste_shortcut_matches_platform_primary_v() {
        #[cfg(target_os = "macos")]
        let modifiers = KeyModifiers::SUPER;
        #[cfg(not(target_os = "macos"))]
        let modifiers = KeyModifiers::CONTROL;

        assert!(is_modal_paste_shortcut(&KeyEvent::new(
            KeyCode::Char('v'),
            modifiers
        )));
        assert!(is_modal_paste_shortcut(&KeyEvent::new(
            KeyCode::Char('V'),
            modifiers | KeyModifiers::SHIFT
        )));
        assert!(!is_modal_paste_shortcut(&KeyEvent::new(
            KeyCode::Char('v'),
            KeyModifiers::ALT
        )));
    }

    #[test]
    fn modal_paste_target_is_active_only_for_text_inputs() {
        let mut state = AppState::test_new();

        state.mode = Mode::RenameTab;
        assert!(modal_paste_target_active(&state));

        state.mode = Mode::Navigator;
        state.navigator.search_focused = false;
        assert!(!modal_paste_target_active(&state));
        state.navigator.search_focused = true;
        assert!(modal_paste_target_active(&state));

        state.mode = Mode::KeybindHelp;
        state.keybind_help.search_focused = false;
        assert!(!modal_paste_target_active(&state));
        state.keybind_help.search_focused = true;
        assert!(modal_paste_target_active(&state));

        state.mode = Mode::ConfirmClose;
        assert!(!modal_paste_target_active(&state));
    }
}
