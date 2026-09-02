use std::{collections::HashMap, ops::Range, time::Duration};

use gpui::{
    Animation, AnimationExt as _, App, AppContext as _, Context, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled as _, Subscription, Window, actions, deferred, div, px,
};
use gpui_base::input::{DiagnosticColors, InputEditorStyle};
use gpui_component::{
    ActiveTheme as _, ElementExt as _, Icon, IconName, Root, Sizable as _, Size, TitleBar,
    WindowExt as _,
    animation::ease_out_cubic,
    button::{Button, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    input::{
        Backspace, DeleteToBeginningOfLine, Editor, EditorState, Input, InputEvent, InputState,
        MoveDown, MoveToEnd, MoveToStart, MoveUp, TextDecorationCollection,
    },
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    v_flex,
};
use uuid::Uuid;

use crate::{
    detection::{DetectedLanguage, LanguageSelection},
    formatting::{FormatOutcome, format_snippet},
    model::{RollData, SnippetData, WorkspaceData, should_delete_snippet_on_backspace},
    persistence::WorkspaceStore,
    tab_drag::{TabDragFinish, TabDragState},
    updater,
};

actions!(
    paperoll,
    [
        NewSnippet,
        NewRoll,
        CloseRoll,
        FormatSnippet,
        DeleteEmptySnippet,
        DeleteEmptySnippetWithCommand,
        NextRoll,
        PreviousRoll,
        MoveToPreviousSnippet,
        MoveToNextSnippet,
        MoveToDocumentStart,
        MoveToDocumentEnd,
        IncreaseFontSize,
        DecreaseFontSize
    ]
);

struct FormatNotification;

const MAX_AUTO_GROW_ROWS: usize = 160;
const MIN_EDITOR_FONT_SIZE: f32 = 10.;
const MAX_EDITOR_FONT_SIZE: f32 = 32.;
const EDITOR_FONT_SIZE_STEP: f32 = 1.;

fn editor_style(focused: bool, cx: &App) -> InputEditorStyle {
    let theme = cx.theme();
    InputEditorStyle {
        foreground: theme.foreground,
        muted_foreground: theme.muted_foreground,
        background: theme
            .highlight_theme
            .style
            .editor_background
            .unwrap_or_else(|| theme.input_background()),
        border: theme.border,
        selection: theme.selection,
        caret: theme.caret,
        diagnostics: DiagnosticColors {
            error: theme.highlight_theme.style.status.error(cx),
            warning: theme.highlight_theme.style.status.warning(cx),
            info: theme.highlight_theme.style.status.info(cx),
            hint: theme.highlight_theme.style.status.hint(cx),
        },
        highlight_styles: theme.highlight_theme.clone(),
        editor_invisible: theme.highlight_theme.style.editor_invisible,
        editor_active_line: focused
            .then_some(theme.highlight_theme.style.editor_active_line)
            .flatten(),
        editor_gutter_background: theme.highlight_theme.style.editor_gutter_background,
        fold_icon_renderer: None,
    }
}

enum UpdateState {
    Checking,
    Available(Box<cargo_packager_updater::Update>),
    Installing,
    Unavailable,
}

struct SnippetPage {
    id: Uuid,
    editor: Entity<EditorState>,
    language: DetectedLanguage,
    language_selection: LanguageSelection,
    jsonl_decorations: Option<TextDecorationCollection>,
    _subscription: Subscription,
}

struct Roll {
    id: Uuid,
    title: String,
    snippets: Vec<SnippetPage>,
}

pub struct Paperoll {
    rolls: Vec<Roll>,
    active_roll_id: Uuid,
    focused_snippet_id: Option<Uuid>,
    focus_handle: FocusHandle,
    page_scroll_handle: ScrollHandle,
    navigator_scroll_handle: ScrollHandle,
    tab_scroll_handle: ScrollHandle,
    tab_drag: TabDragState,
    store: WorkspaceStore,
    persistence_enabled: bool,
    save_error: Option<SharedString>,
    update_state: UpdateState,
}

impl Paperoll {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::app_theme::sync_with_system(Some(window), cx);
        cx.observe_window_appearance(window, |_, window, cx| {
            crate::app_theme::sync_with_system(Some(window), cx);
            cx.notify();
        })
        .detach();
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.restore_focused_editor(window, cx);
                if matches!(this.update_state, UpdateState::Unavailable) {
                    this.check_for_update(cx);
                }
            }
        })
        .detach();

        let store = WorkspaceStore::application_default();
        let (data, persistence_enabled, initial_save_error) = match store.load() {
            Ok(data) => {
                let save_error = store
                    .initialize_if_empty(&data)
                    .err()
                    .map(|error| SharedString::from(format!("Couldn’t save: {error}")));
                (data, true, save_error)
            }
            Err(error) => (
                WorkspaceData::empty(),
                false,
                Some(SharedString::from(format!("Couldn’t load files: {error}"))),
            ),
        };
        let active_roll_id = data.active_roll_id;
        let rolls: Vec<Roll> = data
            .rolls
            .into_iter()
            .map(|roll| Self::build_roll(roll, window, cx))
            .collect();
        let focused_snippet_id = rolls
            .iter()
            .find(|roll| roll.id == active_roll_id)
            .and_then(|roll| roll.snippets.first())
            .map(|snippet| snippet.id);

        let mut paperoll = Self {
            rolls,
            active_roll_id,
            focused_snippet_id,
            focus_handle: cx.focus_handle(),
            page_scroll_handle: ScrollHandle::new(),
            navigator_scroll_handle: ScrollHandle::new(),
            tab_scroll_handle: ScrollHandle::new(),
            tab_drag: TabDragState::default(),
            store,
            persistence_enabled,
            save_error: initial_save_error,
            update_state: UpdateState::Checking,
        };

        paperoll.check_for_update(cx);

        let first_editor = paperoll
            .active_roll()
            .and_then(|roll| roll.snippets.first())
            .map(|snippet| snippet.editor.clone());
        if let Some(editor) = first_editor {
            window.defer(cx, move |window, cx| {
                editor.update(cx, |state, cx| state.focus(window, cx));
            });
        }
        paperoll
    }

    fn check_for_update(&mut self, cx: &mut Context<Self>) {
        self.update_state = UpdateState::Checking;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { updater::check() })
                .await;
            this.update(cx, |this, cx| {
                this.update_state = match result {
                    Ok(Some(update)) => UpdateState::Available(Box::new(update)),
                    Ok(None) => UpdateState::Unavailable,
                    Err(error) => {
                        eprintln!("Couldn’t check for updates: {error}");
                        UpdateState::Unavailable
                    }
                };
                cx.notify();
            })
        })
        .detach();
    }

    fn install_available_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(update) =
            std::mem::replace(&mut self.update_state, UpdateState::Installing)
        else {
            return;
        };
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { updater::install_and_relaunch(*update) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => cx.quit(),
                    Err(error) => {
                        eprintln!("Couldn’t install update: {error}");
                        this.update_state = UpdateState::Checking;
                        this.check_for_update(cx);
                    }
                }
                cx.notify();
            })
        })
        .detach();
    }

    fn build_roll(data: RollData, window: &mut Window, cx: &mut Context<Self>) -> Roll {
        Roll {
            id: data.id,
            title: data.title,
            snippets: data
                .snippets
                .into_iter()
                .map(|snippet| Self::build_snippet(snippet, window, cx))
                .collect(),
        }
    }

    fn build_snippet(
        data: SnippetData,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> SnippetPage {
        let language_selection = LanguageSelection::from_persisted(&data.language);
        let language = language_selection.resolve(&data.text);
        let mut jsonl_decorations = None;
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language(language.highlighter_name())
                .line_number(true)
                .indent_guides(false)
                .folding(false)
                .soft_wrap(true)
                .scroll_beyond_last_line(Some(0))
                .cursor_surrounding_lines(Some(0))
                .placeholder("Write anything…")
                .default_value(data.text.clone());
            if language == DetectedLanguage::JsonLines {
                let decorations = state.create_decorations_collection(Vec::new(), cx);
                state.set_highlighter_factory(
                    crate::jsonl_highlighter::factory(decorations.clone()),
                    cx,
                );
                jsonl_decorations = Some(decorations);
                state.refresh(cx);
            }
            state
        });
        let id = data.id;
        let subscription = cx.subscribe_in(
            &editor,
            window,
            move |this, editor, event: &InputEvent, window, cx| {
                this.on_snippet_event(id, editor, event, window, cx);
            },
        );

        SnippetPage {
            id,
            editor,
            language,
            language_selection,
            jsonl_decorations,
            _subscription: subscription,
        }
    }

    fn on_snippet_event(
        &mut self,
        snippet_id: Uuid,
        editor: &Entity<EditorState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => self.on_snippet_changed(snippet_id, editor, cx),
            InputEvent::Focus => {
                self.focused_snippet_id = Some(snippet_id);
                if let Some((_, snippet_ix)) = self.snippet_position(snippet_id) {
                    self.navigator_scroll_handle.scroll_to_item(snippet_ix);
                }
                cx.notify();
            }
            _ => {}
        }
    }

    fn on_snippet_changed(
        &mut self,
        snippet_id: Uuid,
        editor: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        let text = editor.read(cx).value().to_string();
        let Some((roll_ix, snippet_ix)) = self.snippet_position(snippet_id) else {
            return;
        };

        let detected = self.rolls[roll_ix].snippets[snippet_ix]
            .language_selection
            .resolve(&text);
        let snippet = &mut self.rolls[roll_ix].snippets[snippet_ix];
        if snippet.language != detected {
            snippet.language = detected;
            let decorations = editor.update(cx, |state, cx| {
                let decorations = if detected == DetectedLanguage::JsonLines {
                    let decorations = state.create_decorations_collection(Vec::new(), cx);
                    state.set_highlighter_factory(
                        crate::jsonl_highlighter::factory(decorations.clone()),
                        cx,
                    );
                    Some(decorations)
                } else {
                    None
                };
                state.set_highlighter(detected.highlighter_name(), cx);
                state.refresh(cx);
                decorations
            });
            if detected == DetectedLanguage::JsonLines {
                snippet.jsonl_decorations = decorations;
            } else if let Some(decorations) = snippet.jsonl_decorations.take() {
                decorations.clear(cx);
            }
        }

        let save_result = self.persistence_enabled.then(|| {
            let roll = &self.rolls[roll_ix];
            let snippet = &roll.snippets[snippet_ix];
            self.store.save_snippet(
                roll_ix,
                &roll.title,
                snippet_ix,
                snippet.language_selection,
                &text,
            )
        });
        if let Some(save_result) = save_result {
            self.save_error = save_result
                .err()
                .map(|error| SharedString::from(format!("Couldn’t save: {error}")));
        }
        cx.notify();
    }

    fn on_delete_empty_snippet(
        &mut self,
        _: &DeleteEmptySnippet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.remove_focused_empty_snippet(window, cx) {
            window.dispatch_action(Box::new(Backspace), cx);
        }
    }

    fn on_delete_empty_snippet_with_command(
        &mut self,
        _: &DeleteEmptySnippetWithCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.remove_focused_empty_snippet(window, cx) {
            window.dispatch_action(Box::new(DeleteToBeginningOfLine), cx);
        }
    }

    fn remove_focused_empty_snippet(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let position = self
            .focused_snippet_id
            .and_then(|snippet_id| self.snippet_position(snippet_id));
        let should_delete = position.is_some_and(|(roll_ix, snippet_ix)| {
            let roll = &self.rolls[roll_ix];
            let text = roll.snippets[snippet_ix].editor.read(cx).value();
            should_delete_snippet_on_backspace(&text, roll.snippets.len())
        });

        if let Some((roll_ix, snippet_ix)) = position.filter(|_| should_delete) {
            let snippet_id = self.rolls[roll_ix].snippets[snippet_ix].id;
            self.remove_snippet(snippet_id, window, cx);
            true
        } else {
            false
        }
    }

    fn remove_snippet(&mut self, snippet_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some((roll_ix, snippet_ix)) = self.snippet_position(snippet_id) else {
            return;
        };
        if self.rolls[roll_ix].snippets.len() <= 1 {
            return;
        }

        self.rolls[roll_ix].snippets.remove(snippet_ix);
        let focus_ix = snippet_ix
            .saturating_sub(1)
            .min(self.rolls[roll_ix].snippets.len() - 1);
        let editor = self.rolls[roll_ix].snippets[focus_ix].editor.clone();
        self.focused_snippet_id = Some(self.rolls[roll_ix].snippets[focus_ix].id);
        self.scroll_page_to_snippet(focus_ix, window, cx);
        self.navigator_scroll_handle.scroll_to_item(focus_ix);
        editor.update(cx, |state, cx| state.focus(window, cx));
        self.save(cx);
        cx.notify();
    }

    fn add_snippet_below_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(roll_ix) = self.active_roll_ix() else {
            return;
        };
        let insertion_ix = self
            .focused_snippet_id
            .and_then(|id| {
                self.rolls[roll_ix]
                    .snippets
                    .iter()
                    .position(|snippet| snippet.id == id)
            })
            .map_or(self.rolls[roll_ix].snippets.len(), |ix| ix + 1);
        let snippet = Self::build_snippet(SnippetData::empty(), window, cx);
        let id = snippet.id;
        let editor = snippet.editor.clone();
        self.rolls[roll_ix].snippets.insert(insertion_ix, snippet);
        self.focused_snippet_id = Some(id);
        self.scroll_page_to_snippet(insertion_ix, window, cx);
        self.navigator_scroll_handle.scroll_to_item(insertion_ix);
        editor.update(cx, |state, cx| state.focus(window, cx));
        self.save(cx);
        cx.notify();
    }

    fn add_roll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let data = RollData::empty(next_roll_number(
            self.rolls.iter().map(|roll| roll.title.as_str()),
        ));
        let roll = Self::build_roll(data, window, cx);
        self.active_roll_id = roll.id;
        let editor = roll.snippets[0].editor.clone();
        self.focused_snippet_id = Some(roll.snippets[0].id);
        self.rolls.push(roll);
        self.scroll_page_to_snippet(0, window, cx);
        self.navigator_scroll_handle.scroll_to_item(0);
        editor.update(cx, |state, cx| state.focus(window, cx));
        self.save(cx);
        cx.notify();
    }

    fn close_roll(&mut self, roll_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(roll_ix) = self.rolls.iter().position(|roll| roll.id == roll_id) else {
            return;
        };

        self.rolls.remove(roll_ix);
        if self.rolls.is_empty() {
            self.rolls
                .push(Self::build_roll(RollData::empty(1), window, cx));
        }
        let active_ix = roll_ix.min(self.rolls.len() - 1);
        self.active_roll_id = self.rolls[active_ix].id;
        let first = &self.rolls[active_ix].snippets[0];
        self.focused_snippet_id = Some(first.id);
        let editor = first.editor.clone();
        self.scroll_page_to_snippet(0, window, cx);
        self.navigator_scroll_handle.scroll_to_item(0);
        editor.update(cx, |state, cx| state.focus(window, cx));
        self.save(cx);
        cx.notify();
    }

    fn request_close_roll(&mut self, roll_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(title) = self
            .rolls
            .iter()
            .find(|roll| roll.id == roll_id)
            .map(|roll| roll.title.clone())
        else {
            return;
        };
        let owner = cx.weak_entity();

        window.open_dialog(cx, move |dialog, _, _| {
            let cancel = Button::new("cancel-close-roll")
                .label("Cancel")
                .on_click(|_, window, cx| window.close_dialog(cx));
            let confirm_owner = owner.clone();
            let confirm = Button::new("confirm-close-roll")
                .label("Close roll")
                .danger()
                .on_click(move |_, window, cx| {
                    _ = confirm_owner.update(cx, |this, cx| {
                        this.close_roll(roll_id, window, cx);
                    });
                    window.close_dialog(cx);
                });

            dialog
                .title(format!("Close {title}?"))
                .child("The roll will be removed from this workspace. This cannot be undone.")
                .footer(h_flex().justify_end().gap_2().child(cancel).child(confirm))
                .on_ok({
                    let owner = owner.clone();
                    move |_, window, cx| {
                        _ = owner.update(cx, |this, cx| {
                            this.close_roll(roll_id, window, cx);
                        });
                        true
                    }
                })
        });
    }

    fn rename_roll(&mut self, roll_id: Uuid, title: &str, cx: &mut Context<Self>) -> bool {
        let title = title.trim();
        if title.is_empty() {
            return false;
        }

        let Some(roll) = self.rolls.iter_mut().find(|roll| roll.id == roll_id) else {
            return true;
        };
        if roll.title == title {
            return true;
        }

        roll.title = title.to_string();
        self.save(cx);
        cx.notify();
        true
    }

    fn request_rename_roll(&mut self, roll_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(title) = self
            .rolls
            .iter()
            .find(|roll| roll.id == roll_id)
            .map(|roll| roll.title.clone())
        else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title));
        let focus_input = input.clone();
        let owner = cx.weak_entity();

        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Rename roll")
                .child(Input::new(&input).w_full())
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Rename")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok({
                    let input = input.clone();
                    let owner = owner.clone();
                    move |_, _, cx| {
                        let title = input.read(cx).value();
                        owner
                            .update(cx, |this, cx| this.rename_roll(roll_id, &title, cx))
                            .unwrap_or(true)
                    }
                })
        });
        window.defer(cx, move |window, cx| {
            focus_input.update(cx, |state, cx| {
                state.focus(window, cx);
                state.select_all(window, cx);
            });
        });
    }

    fn select_roll(&mut self, roll_id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_roll_id == roll_id {
            return;
        }
        self.active_roll_id = roll_id;
        let first = self
            .active_roll()
            .and_then(|roll| roll.snippets.first())
            .map(|snippet| (snippet.id, snippet.editor.clone()));
        if let Some((id, editor)) = first {
            self.focused_snippet_id = Some(id);
            self.scroll_page_to_snippet(0, window, cx);
            self.navigator_scroll_handle.scroll_to_item(0);
            editor.update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
    }

    fn select_relative_roll(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_ix) = self
            .rolls
            .iter()
            .position(|roll| roll.id == self.active_roll_id)
        else {
            return;
        };
        if self.rolls.len() < 2 {
            return;
        }

        let next_ix = (active_ix as isize + direction).rem_euclid(self.rolls.len() as isize);
        let roll_id = self.rolls[next_ix as usize].id;
        self.select_roll(roll_id, window, cx);
    }

    fn on_next_roll(&mut self, _: &NextRoll, window: &mut Window, cx: &mut Context<Self>) {
        self.select_relative_roll(1, window, cx);
    }

    fn on_previous_roll(&mut self, _: &PreviousRoll, window: &mut Window, cx: &mut Context<Self>) {
        self.select_relative_roll(-1, window, cx);
    }

    fn focused_editor(&self) -> Option<Entity<EditorState>> {
        let (roll_ix, snippet_ix) = self
            .focused_snippet_id
            .and_then(|snippet_id| self.snippet_position(snippet_id))?;
        Some(self.rolls[roll_ix].snippets[snippet_ix].editor.clone())
    }

    fn restore_focused_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.focused_editor() else {
            return;
        };
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| state.focus(window, cx));
        });
    }

    fn focus_adjacent_snippet(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((roll_ix, snippet_ix)) = self
            .focused_snippet_id
            .and_then(|snippet_id| self.snippet_position(snippet_id))
        else {
            return false;
        };
        if self.rolls[roll_ix].id != self.active_roll_id {
            return false;
        }

        let target_ix = snippet_ix as isize + direction;
        if !(0..self.rolls[roll_ix].snippets.len() as isize).contains(&target_ix) {
            return false;
        }

        let snippet_id = self.rolls[roll_ix].snippets[target_ix as usize].id;
        self.navigate_to_snippet(snippet_id, window, cx);
        true
    }

    fn defer_adjacent_snippet_if_unchanged(
        &mut self,
        snippet_id: Uuid,
        editor: Entity<EditorState>,
        before: (usize, Range<usize>),
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.defer_in(window, move |this, window, cx| {
            if this.focused_snippet_id != Some(snippet_id) {
                return;
            }
            let unchanged = {
                let state = editor.read(cx);
                state.cursor() == before.0 && state.selected_range() == before.1
            };
            if !unchanged || !this.focus_adjacent_snippet(direction, window, cx) {
                return;
            }

            if direction < 0 {
                window.dispatch_action(Box::new(MoveToEnd), cx);
            } else {
                window.dispatch_action(Box::new(MoveToStart), cx);
            }
        });
    }

    fn on_move_to_previous_snippet(
        &mut self,
        _: &MoveToPreviousSnippet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.focused_editor() else {
            return;
        };
        let Some(snippet_id) = self.focused_snippet_id else {
            return;
        };
        let before = {
            let state = editor.read(cx);
            (state.cursor(), state.selected_range())
        };
        window.dispatch_action(Box::new(MoveUp), cx);
        self.defer_adjacent_snippet_if_unchanged(snippet_id, editor, before, -1, window, cx);
    }

    fn on_move_to_next_snippet(
        &mut self,
        _: &MoveToNextSnippet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.focused_editor() else {
            return;
        };
        let Some(snippet_id) = self.focused_snippet_id else {
            return;
        };
        let before = {
            let state = editor.read(cx);
            (state.cursor(), state.selected_range())
        };
        window.dispatch_action(Box::new(MoveDown), cx);
        self.defer_adjacent_snippet_if_unchanged(snippet_id, editor, before, 1, window, cx);
    }

    fn on_move_to_document_end(
        &mut self,
        _: &MoveToDocumentEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(snippet_id) = self.focused_snippet_id else {
            return;
        };
        let Some((roll_ix, snippet_ix)) = self.snippet_position(snippet_id) else {
            return;
        };
        if self.rolls[roll_ix].id != self.active_roll_id {
            return;
        }
        let snippet_count = self.rolls[roll_ix].snippets.len();

        window.dispatch_action(Box::new(MoveToEnd), cx);
        let scroll_handle = self.page_scroll_handle.clone();
        let owner = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            // Let GPUI's editor finish following the cursor before moving the
            // enclosing roll viewport.
            window.dispatch_action(Box::new(MoveToEnd), cx);
            if snippet_ix + 1 < snippet_count {
                scroll_handle.scroll_to_top_of_item(snippet_ix + 1);
            } else {
                scroll_handle.scroll_to_bottom();
            }

            let settled_scroll_handle = scroll_handle.clone();
            window.defer(cx, move |window, cx| {
                window.dispatch_action(Box::new(MoveToEnd), cx);
                if snippet_ix + 1 < snippet_count {
                    // The next page is now at the viewport top. Move upward by
                    // one viewport so the focused page ends at its bottom.
                    let mut offset = settled_scroll_handle.offset();
                    offset.y += settled_scroll_handle.bounds().size.height;
                    settled_scroll_handle.set_offset(offset);
                } else {
                    settled_scroll_handle.scroll_to_bottom();
                }
                _ = owner.update(cx, |_, cx| cx.notify());
            });
        });
        cx.notify();
    }

    fn on_move_to_document_start(
        &mut self,
        _: &MoveToDocumentStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(snippet_id) = self.focused_snippet_id else {
            return;
        };
        let Some((roll_ix, snippet_ix)) = self.snippet_position(snippet_id) else {
            return;
        };
        if self.rolls[roll_ix].id != self.active_roll_id {
            return;
        }

        window.dispatch_action(Box::new(MoveToStart), cx);
        self.page_scroll_handle.scroll_to_top_of_item(snippet_ix);

        let scroll_handle = self.page_scroll_handle.clone();
        let owner = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            window.dispatch_action(Box::new(MoveToStart), cx);
            scroll_handle.scroll_to_top_of_item(snippet_ix);
            _ = owner.update(cx, |_, cx| cx.notify());
        });
        cx.notify();
    }

    fn adjust_editor_font_size(&mut self, delta: f32, cx: &mut Context<Self>) {
        let current = gpui_component::Theme::global(cx).mono_font_size.as_f32();
        let next = (current + delta).clamp(MIN_EDITOR_FONT_SIZE, MAX_EDITOR_FONT_SIZE);
        if next == current {
            return;
        }

        gpui_component::Theme::global_mut(cx).mono_font_size = px(next);
        for roll in &self.rolls {
            for snippet in &roll.snippets {
                snippet.editor.update(cx, |_, cx| cx.notify());
            }
        }
        cx.notify();
    }

    fn on_increase_font_size(
        &mut self,
        _: &IncreaseFontSize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adjust_editor_font_size(EDITOR_FONT_SIZE_STEP, cx);
    }

    fn on_decrease_font_size(
        &mut self,
        _: &DecreaseFontSize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adjust_editor_font_size(-EDITOR_FONT_SIZE_STEP, cx);
    }

    fn navigate_to_snippet(
        &mut self,
        snippet_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((roll_ix, snippet_ix)) = self.snippet_position(snippet_id) else {
            return;
        };
        if self.rolls[roll_ix].id != self.active_roll_id {
            return;
        }

        self.focused_snippet_id = Some(snippet_id);
        self.scroll_page_to_snippet(snippet_ix, window, cx);
        self.navigator_scroll_handle.scroll_to_item(snippet_ix);
        let editor = self.rolls[roll_ix].snippets[snippet_ix].editor.clone();
        editor.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    fn set_snippet_language(
        &mut self,
        snippet_id: Uuid,
        selection: LanguageSelection,
        cx: &mut Context<Self>,
    ) {
        let Some((roll_ix, snippet_ix)) = self.snippet_position(snippet_id) else {
            return;
        };
        let snippet = &mut self.rolls[roll_ix].snippets[snippet_ix];
        let text = snippet.editor.read(cx).value().to_string();
        let language = selection.resolve(&text);
        snippet.language_selection = selection;
        snippet.language = language;
        let decorations = snippet.editor.update(cx, |state, cx| {
            let decorations = if language == DetectedLanguage::JsonLines {
                let decorations = state.create_decorations_collection(Vec::new(), cx);
                state.set_highlighter_factory(
                    crate::jsonl_highlighter::factory(decorations.clone()),
                    cx,
                );
                Some(decorations)
            } else {
                None
            };
            state.set_highlighter(language.highlighter_name(), cx);
            state.refresh(cx);
            decorations
        });
        if language == DetectedLanguage::JsonLines {
            snippet.jsonl_decorations = decorations;
        } else if let Some(decorations) = snippet.jsonl_decorations.take() {
            decorations.clear(cx);
        }
        self.save(cx);
        cx.notify();
    }

    fn begin_tab_drag(&mut self, roll_id: Uuid, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let canonical = self.rolls.iter().map(|roll| roll.id).collect();
        self.tab_drag.begin(roll_id, event.position.x, canonical);
        cx.stop_propagation();
    }

    fn update_tab_drag(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.tab_drag.update(event.position.x) {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn finish_tab_drag(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(result) = self.tab_drag.finish() else {
            return;
        };
        cx.stop_propagation();

        match result {
            TabDragFinish::Click(roll_id) => self.select_roll(roll_id, window, cx),
            TabDragFinish::Reorder(order) => {
                let mut rolls_by_id: HashMap<_, _> = std::mem::take(&mut self.rolls)
                    .into_iter()
                    .map(|roll| (roll.id, roll))
                    .collect();
                self.rolls = order
                    .into_iter()
                    .filter_map(|id| rolls_by_id.remove(&id))
                    .collect();
                self.save(cx);
                cx.notify();
            }
        }
    }

    fn characters_per_line(&self) -> usize {
        let width = self.page_scroll_handle.bounds().size.width.as_f32();
        (((width - 48.).max(320.) / 9.5).floor() as usize).max(32)
    }

    fn scroll_page_to_snippet(
        &mut self,
        snippet_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.page_scroll_handle.scroll_to_top_of_item(snippet_ix);

        // Focus and editor-height updates can relayout the roll in the same
        // frame. Re-issue the jump after that layout so long rolls cannot
        // consume it against stale child bounds.
        let scroll_handle = self.page_scroll_handle.clone();
        let owner = cx.weak_entity();
        window.defer(cx, move |_, cx| {
            scroll_handle.scroll_to_top_of_item(snippet_ix);
            _ = owner.update(cx, |_, cx| cx.notify());
        });
    }

    fn on_new_snippet(&mut self, _: &NewSnippet, window: &mut Window, cx: &mut Context<Self>) {
        self.add_snippet_below_focused(window, cx);
    }

    fn on_new_roll(&mut self, _: &NewRoll, window: &mut Window, cx: &mut Context<Self>) {
        self.add_roll(window, cx);
    }

    fn on_close_roll(&mut self, _: &CloseRoll, window: &mut Window, cx: &mut Context<Self>) {
        self.request_close_roll(self.active_roll_id, window, cx);
    }

    fn on_format_snippet(
        &mut self,
        _: &FormatSnippet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(snippet_id) = self.focused_snippet_id else {
            return;
        };
        let Some((roll_ix, snippet_ix)) = self.snippet_position(snippet_id) else {
            return;
        };
        if self.rolls[roll_ix].id != self.active_roll_id {
            return;
        }

        let snippet = &self.rolls[roll_ix].snippets[snippet_ix];
        let editor = snippet.editor.clone();
        let language = snippet.language;
        let text = editor.read(cx).value().to_string();
        let cursor = editor.read(cx).cursor();

        match format_snippet(&text, language) {
            Ok(FormatOutcome::Formatted(formatted)) => {
                let mapped_cursor = remap_cursor(&text, &formatted, cursor);
                editor.update(cx, |state, cx| {
                    state.replace_all(formatted, window, cx);
                    state.set_selected_range(mapped_cursor..mapped_cursor, cx);
                    state.focus(window, cx);
                });
                window.push_notification(
                    Notification::success(format!("Formatted {} snippet", language.label()))
                        .id::<FormatNotification>(),
                    cx,
                );
            }
            Ok(FormatOutcome::Unchanged) => {
                window.push_notification(
                    Notification::info(format!(
                        "{} snippet is already formatted",
                        language.label()
                    ))
                    .id::<FormatNotification>(),
                    cx,
                );
            }
            Ok(FormatOutcome::Unsupported) => {
                window.push_notification(
                    Notification::warning(format!(
                        "No formatter available for {} snippets",
                        language.label()
                    ))
                    .id::<FormatNotification>(),
                    cx,
                );
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(format!(
                        "Couldn’t format {}: {}",
                        language.label(),
                        concise_format_error(&error)
                    ))
                    .id::<FormatNotification>(),
                    cx,
                );
            }
        }
    }

    fn active_roll_ix(&self) -> Option<usize> {
        self.rolls
            .iter()
            .position(|roll| roll.id == self.active_roll_id)
    }

    fn active_roll(&self) -> Option<&Roll> {
        self.active_roll_ix().map(|ix| &self.rolls[ix])
    }

    fn snippet_position(&self, snippet_id: Uuid) -> Option<(usize, usize)> {
        self.rolls.iter().enumerate().find_map(|(roll_ix, roll)| {
            roll.snippets
                .iter()
                .position(|snippet| snippet.id == snippet_id)
                .map(|snippet_ix| (roll_ix, snippet_ix))
        })
    }

    fn snapshot(&self, cx: &App) -> WorkspaceData {
        WorkspaceData {
            active_roll_id: self.active_roll_id,
            rolls: self
                .rolls
                .iter()
                .map(|roll| RollData {
                    id: roll.id,
                    title: roll.title.clone(),
                    snippets: roll
                        .snippets
                        .iter()
                        .map(|snippet| SnippetData {
                            id: snippet.id,
                            text: snippet.editor.read(cx).value().to_string(),
                            language: snippet.language_selection.persisted().to_string(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn save(&mut self, cx: &App) {
        if !self.persistence_enabled {
            return;
        }
        self.save_error = self
            .store
            .save(&self.snapshot(cx))
            .err()
            .map(|error| SharedString::from(format!("Couldn’t save: {error}")));
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let order = self.tab_drag.order(self.rolls.iter().map(|roll| roll.id));
        let tabs = order.into_iter().filter_map(|roll_id| {
            let roll = self.rolls.iter().find(|roll| roll.id == roll_id)?;
            let title = roll.title.clone();
            let selected = roll_id == self.active_roll_id;
            let drag_offset = self.tab_drag.drag_offset(roll_id);
            let settling = self.tab_drag.settling_offset(roll_id);
            let measuring_view = view.clone();
            let rename_view = view.clone();

            let visual = h_flex()
                .relative()
                .h_full()
                .flex_none()
                .border_b_2()
                .border_color(if selected {
                    cx.theme().primary
                } else {
                    gpui::transparent_black()
                })
                .bg(if selected {
                    cx.theme().background
                } else {
                    *cx.theme().tokens.tab_bar
                })
                .text_color(if selected {
                    cx.theme().foreground
                } else {
                    cx.theme().muted_foreground
                })
                .child(
                    h_flex()
                        .id(format!("tab-label-{roll_id}"))
                        .h_full()
                        .px_3()
                        .gap_2()
                        .cursor_grab()
                        .child(div().size_1_5().rounded_full().bg(cx.theme().primary))
                        .child(title.clone())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                if event.click_count == 2 {
                                    cx.stop_propagation();
                                    this.request_rename_roll(roll_id, window, cx);
                                } else {
                                    this.begin_tab_drag(roll_id, event, cx);
                                }
                            }),
                        )
                        .context_menu(move |menu, window, _| {
                            menu.item(PopupMenuItem::new("Rename…").on_click(window.listener_for(
                                &rename_view,
                                move |this, _, window, cx| {
                                    this.request_rename_roll(roll_id, window, cx);
                                },
                            )))
                        }),
                )
                .child(
                    Button::new(format!("close-roll-{roll_id}"))
                        .xsmall()
                        .ghost()
                        .icon(Icon::new(IconName::Close))
                        .tooltip("Close roll (⌘W)")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.request_close_roll(roll_id, window, cx);
                        })),
                );

            let visual = if let Some(offset) = drag_offset {
                // A translated tab can overlap siblings that appear later in layout order.
                // Paint the dragged tab after the regular tab row so it always stays on top.
                deferred(visual.left(offset).shadow_md()).into_any_element()
            } else if let Some((offset, epoch)) = settling {
                visual
                    .with_animation(
                        format!("settle-tab-{roll_id}-{epoch}"),
                        Animation::new(Duration::from_millis(160)).with_easing(ease_out_cubic),
                        move |this, delta| this.left(offset * (1. - delta)),
                    )
                    .into_any_element()
            } else {
                visual.into_any_element()
            };

            Some(
                h_flex()
                    .id(format!("tab-slot-{roll_id}"))
                    .h_full()
                    .flex_none()
                    .on_prepaint(move |bounds, _, cx| {
                        measuring_view.update(cx, |this, _| {
                            this.tab_drag.record_frame(roll_id, bounds);
                        });
                    })
                    .child(visual),
            )
        });

        h_flex()
            .size_full()
            .min_w_0()
            .child(
                div()
                    .id("paperoll-tabs")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_x_scroll()
                    .track_scroll(&self.tab_scroll_handle)
                    .child(h_flex().h_full().children(tabs)),
            )
            .child(
                Button::new("new-roll")
                    .small()
                    .ghost()
                    .mr_2()
                    .icon(IconName::Plus)
                    .tooltip("New roll (⌘T)")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_roll(window, cx);
                    })),
            )
    }

    fn render_navigator(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.active_roll().into_iter().flat_map(|roll| {
            roll.snippets.iter().enumerate().map(|(ix, snippet)| {
                let snippet_id = snippet.id;
                let active = self.focused_snippet_id == Some(snippet_id);
                h_flex()
                    .id(format!("navigate-snippet-{snippet_id}"))
                    .h_9()
                    .w_full()
                    .px_3()
                    .gap_2()
                    .cursor_pointer()
                    .bg(if active {
                        *cx.theme().tokens.secondary
                    } else {
                        gpui::transparent_black()
                    })
                    .hover(|this| this.bg(cx.theme().tokens.secondary_hover))
                    .child(
                        div()
                            .size_2()
                            .rounded_full()
                            .border_1()
                            .border_color(if active {
                                cx.theme().primary
                            } else {
                                cx.theme().border
                            })
                            .bg(if active {
                                cx.theme().primary
                            } else {
                                gpui::transparent_black()
                            }),
                    )
                    .child(format!("{:03}", ix + 1))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate_to_snippet(snippet_id, window, cx);
                    }))
            })
        });

        v_flex()
            .id("snippet-navigator")
            .w_20()
            .h_full()
            .flex_none()
            .overflow_y_scroll()
            .track_scroll(&self.navigator_scroll_handle)
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().muted)
            .font_family(cx.theme().mono_font_family.clone())
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .children(items)
    }

    fn render_pages(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let characters_per_line = self.characters_per_line();
        let view = cx.entity();
        let snippets = self.active_roll().into_iter().flat_map(|roll| {
            roll.snippets.iter().enumerate().map(|(ix, snippet)| {
                let (rows, editor_line_height) = {
                    let editor = snippet.editor.read(cx);
                    let measured_line_height = editor
                        .line_height()
                        .unwrap_or_else(|| px((cx.theme().mono_font_size.as_f32() * 1.5).round()));
                    (
                        editor_visual_rows_capped(
                            &editor.value(),
                            characters_per_line,
                            MAX_AUTO_GROW_ROWS,
                        ),
                        measured_line_height,
                    )
                };
                let editor_height = editor_line_height * rows as f32 + Size::Medium.input_py() * 2.;
                let focused = self.focused_snippet_id == Some(snippet.id);
                let snippet_id = snippet.id;
                let selection = snippet.language_selection;
                let language = snippet.language;
                let language_label = match selection {
                    LanguageSelection::Auto => format!("Auto · {}", language.label()),
                    LanguageSelection::Explicit(language) => language.label().to_string(),
                };
                let menu_view = view.clone();
                let alternate_background = cx.theme().background.blend(
                    cx.theme()
                        .foreground
                        .opacity(if cx.theme().is_dark() { 0.055 } else { 0.035 }),
                );
                let block_background = if ix % 2 == 0 {
                    cx.theme().background
                } else {
                    alternate_background
                };
                let block_padding_bottom = if language == DetectedLanguage::Markdown {
                    px(0.)
                } else {
                    px(4.)
                };
                let block_editor = snippet.editor.clone();
                let style_editor = snippet.editor.clone();

                v_flex()
                    .id(format!("snippet-page-{}", snippet.id))
                    .relative()
                    .w_full()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(block_background)
                    .pt(px(8.))
                    .pb(block_padding_bottom)
                    .cursor_text()
                    .on_prepaint(move |_, _, cx| {
                        let style = editor_style(focused, cx);
                        style_editor.update(cx, |state, _| state.set_editor_style(style));
                    })
                    .on_click(move |_, window, cx| {
                        block_editor.update(cx, |state, cx| state.focus(window, cx));
                    })
                    .child(
                        div()
                            .absolute()
                            .left(px(5.))
                            .top(px(17.))
                            .w(px(6.))
                            .h(px(6.))
                            .rounded_full()
                            .bg(if focused {
                                cx.theme().primary
                            } else {
                                gpui::transparent_black()
                            }),
                    )
                    .child(
                        h_flex()
                            // GPUI Component's editor reserves 10pt input padding and
                            // a 10pt code gutter. The compact picker supplies 4pt itself.
                            .pl(px(16.))
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                Button::new(format!("language-{snippet_id}"))
                                    .xsmall()
                                    .ghost()
                                    .compact()
                                    .label(language_label)
                                    .dropdown_caret(true)
                                    .dropdown_menu(move |menu, window, _| {
                                        let auto_view = menu_view.clone();
                                        let menu = menu
                                            .scrollable(true)
                                            .max_h(px(420.))
                                            .min_w(px(190.))
                                            .label("Highlight language")
                                            .item(
                                                PopupMenuItem::new(format!(
                                                    "Auto · {}",
                                                    language.label()
                                                ))
                                                .checked(matches!(
                                                    selection,
                                                    LanguageSelection::Auto
                                                ))
                                                .on_click(window.listener_for(
                                                    &auto_view,
                                                    move |this, _, _, cx| {
                                                        this.set_snippet_language(
                                                            snippet_id,
                                                            LanguageSelection::Auto,
                                                            cx,
                                                        );
                                                    },
                                                )),
                                            )
                                            .separator();

                                        DetectedLanguage::ALL.into_iter().fold(
                                            menu,
                                            |menu, candidate| {
                                                let item_view = menu_view.clone();
                                                menu.item(
                                                    PopupMenuItem::new(candidate.label())
                                                        .checked(
                                                            selection
                                                                == LanguageSelection::Explicit(
                                                                    candidate,
                                                                ),
                                                        )
                                                        .on_click(window.listener_for(
                                                            &item_view,
                                                            move |this, _, _, cx| {
                                                                this.set_snippet_language(
                                                                    snippet_id,
                                                                    LanguageSelection::Explicit(
                                                                        candidate,
                                                                    ),
                                                                    cx,
                                                                );
                                                            },
                                                        )),
                                                )
                                            },
                                        )
                                    }),
                            ),
                    )
                    .child(
                        Editor::new(&snippet.editor)
                            .h(editor_height)
                            .ml(-Size::Medium.input_px())
                            .appearance(false)
                            .bordered(false)
                            .aria_label(format!("Page {} editor", ix + 1)),
                    )
            })
        });

        div()
            .id("paperoll-pages")
            .h_full()
            .min_h_0()
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.page_scroll_handle)
            .bg(cx.theme().background)
            .children(snippets)
    }

    fn render_snippets(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .min_h_0()
            .flex_1()
            .items_start()
            .child(self.render_navigator(cx))
            .child(self.render_pages(cx))
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let page_count = self.active_roll().map_or(0, |roll| roll.snippets.len());
        let status = self.save_error.as_ref().map_or_else(
            || format!("{page_count} pages"),
            |error| format!("{page_count} pages · {error}"),
        );
        let update_label = match &self.update_state {
            UpdateState::Available(update) => Some(format!("Update to {}", update.version)),
            _ => None,
        };
        let shortcuts = h_flex()
            .gap_3()
            .children(update_label.map(|label| {
                Button::new("install-update")
                    .label(label)
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.install_available_update(cx);
                    }))
            }))
            .child("⌥⇧F Format snippet   ⌘↩ New page   ⌘T New roll");

        h_flex()
            .w_full()
            .flex_none()
            .px_4()
            .py_2()
            .justify_between()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().muted)
            .text_xs()
            .text_color(if self.save_error.is_some() {
                cx.theme().danger
            } else {
                cx.theme().muted_foreground
            })
            .child(status)
            .child(shortcuts)
    }
}

fn editor_visual_rows_capped(text: &str, characters_per_line: usize, maximum_rows: usize) -> usize {
    let characters_per_line = characters_per_line.max(1);
    let maximum_rows = maximum_rows.max(1);
    let mut rows = 0;
    for line in text.split('\n') {
        let remaining_rows = maximum_rows - rows;
        let maximum_characters = remaining_rows.saturating_mul(characters_per_line);
        let characters = line.chars().take(maximum_characters).count().max(1);
        rows += characters.div_ceil(characters_per_line);
        if rows >= maximum_rows {
            return maximum_rows;
        }
    }
    rows.max(1)
}

fn remap_cursor(before: &str, after: &str, cursor: usize) -> usize {
    let cursor = cursor.min(before.len());
    if cursor == 0 || after.is_empty() {
        return 0;
    }
    if cursor == before.len() {
        return after.len();
    }

    let mut prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while !before.is_char_boundary(prefix) || !after.is_char_boundary(prefix) {
        prefix -= 1;
    }
    if cursor <= prefix {
        return cursor;
    }

    let before_suffix_end = if before.ends_with('\n') && !after.ends_with('\n') {
        before.len() - 1
    } else {
        before.len()
    };
    let after_suffix_end = if after.ends_with('\n') && !before.ends_with('\n') {
        after.len() - 1
    } else {
        after.len()
    };
    let max_suffix = (before_suffix_end - prefix).min(after_suffix_end - prefix);
    let mut suffix = before[..before_suffix_end]
        .bytes()
        .rev()
        .zip(after[..after_suffix_end].bytes().rev())
        .take(max_suffix)
        .take_while(|(left, right)| left == right)
        .count();
    while !before.is_char_boundary(before_suffix_end - suffix)
        || !after.is_char_boundary(after_suffix_end - suffix)
    {
        suffix -= 1;
    }

    let unchanged_suffix_start = before_suffix_end - suffix;
    if cursor >= unchanged_suffix_start {
        return after_suffix_end - (before_suffix_end - cursor);
    }

    let before_middle_len = unchanged_suffix_start - prefix;
    let after_middle_len = after_suffix_end - suffix - prefix;
    let middle_offset = cursor - prefix;
    let mut mapped = prefix + middle_offset * after_middle_len / before_middle_len.max(1);
    while !after.is_char_boundary(mapped) {
        mapped -= 1;
    }
    mapped
}

fn concise_format_error(error: &str) -> String {
    let first_line = error
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("invalid syntax")
        .trim();
    let mut concise: String = first_line.chars().take(160).collect();
    if first_line.chars().count() > 160 {
        concise.push('…');
    }
    concise
}

fn next_roll_number<'a>(titles: impl IntoIterator<Item = &'a str>) -> usize {
    let titles = titles.into_iter().collect::<Vec<_>>();
    let mut candidate = titles
        .last()
        .and_then(|title| title.strip_prefix("Roll "))
        .and_then(|number| number.parse::<usize>().ok())
        .map_or(titles.len() + 1, |number| number + 1);

    while titles
        .iter()
        .any(|title| *title == format!("Roll {candidate}"))
    {
        candidate += 1;
    }
    candidate
}

impl Focusable for Paperoll {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Paperoll {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("Paperoll")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_snippet))
            .on_action(cx.listener(Self::on_new_roll))
            .on_action(cx.listener(Self::on_close_roll))
            .on_action(cx.listener(Self::on_format_snippet))
            .on_action(cx.listener(Self::on_delete_empty_snippet))
            .on_action(cx.listener(Self::on_delete_empty_snippet_with_command))
            .on_action(cx.listener(Self::on_next_roll))
            .on_action(cx.listener(Self::on_previous_roll))
            .on_action(cx.listener(Self::on_move_to_previous_snippet))
            .on_action(cx.listener(Self::on_move_to_next_snippet))
            .on_action(cx.listener(Self::on_move_to_document_start))
            .on_action(cx.listener(Self::on_move_to_document_end))
            .on_action(cx.listener(Self::on_increase_font_size))
            .on_action(cx.listener(Self::on_decrease_font_size))
            .on_mouse_move(cx.listener(Self::update_tab_drag))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::finish_tab_drag))
            .size_full()
            .min_h_0()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new()
                    .bg(cx.theme().title_bar)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(self.render_tab_bar(cx)),
            )
            .child(self.render_snippets(cx))
            .child(self.render_status_bar(cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{concise_format_error, editor_visual_rows_capped, next_roll_number, remap_cursor};

    #[test]
    fn new_roll_number_follows_the_latest_remaining_default_name() {
        assert_eq!(next_roll_number(["Roll 1"]), 2);
        assert_eq!(next_roll_number(["Roll 1", "Roll 2", "Roll 3"]), 4);
        assert_eq!(next_roll_number(["Roll 1", "Roll 3", "Roll 2"]), 4);
    }

    #[test]
    fn editor_rows_match_content_and_soft_wraps_without_an_artificial_row() {
        assert_eq!(editor_visual_rows_capped("", 10, usize::MAX), 1);
        assert_eq!(editor_visual_rows_capped("one", 10, usize::MAX), 1);
        assert_eq!(editor_visual_rows_capped("one\ntwo\n", 10, usize::MAX), 3);
        assert_eq!(
            editor_visual_rows_capped(&"x".repeat(101), 10, usize::MAX),
            11
        );
        assert_eq!(editor_visual_rows_capped(&"x".repeat(10_000), 10, 160), 160);
    }

    #[test]
    fn formatting_cursor_keeps_stable_edges_and_suffixes() {
        assert_eq!(remap_cursor("abc", "a b c\n", 0), 0);
        assert_eq!(remap_cursor("abc", "a b c\n", 3), 6);
        assert_eq!(
            remap_cursor("let  name = value", "let name = value\n", 12),
            11
        );
        assert_eq!(remap_cursor("λ=1", "λ = 1\n", 2), 2);
    }

    #[test]
    fn formatting_errors_stay_compact_for_notifications() {
        assert_eq!(
            concise_format_error("\nUnexpected token\nline 2 details"),
            "Unexpected token"
        );
        assert_eq!(concise_format_error("\n\n"), "invalid syntax");
    }
}
