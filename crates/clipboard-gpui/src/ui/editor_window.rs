use super::todo_editor::TodoDraft;
use super::*;

pub(super) const CONTEXT: &str = "ContentEditor";
const EDITOR_WIDTH: f32 = 640.;
const EDITOR_HEIGHT: f32 = 480.;
const EDITOR_MIN_WIDTH: f32 = 420.;
const EDITOR_MIN_HEIGHT: f32 = 280.;

actions!(content_editor, [SaveContentEditor, CancelContentEditor]);

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelContentEditor, Some(CONTEXT)),
        KeyBinding::new("ctrl-enter", SaveContentEditor, Some(CONTEXT)),
        KeyBinding::new("ctrl-s", SaveContentEditor, Some(CONTEXT)),
    ]);
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-enter", SaveContentEditor, Some(CONTEXT)),
        KeyBinding::new("cmd-s", SaveContentEditor, Some(CONTEXT)),
    ]);
}

pub(super) fn editor_title(language: LanguagePreference, memo: Option<TodoDraft>) -> &'static str {
    match memo {
        Some(TodoDraft::New(_)) => tr(language, "添加备忘录", "Add memo"),
        Some(TodoDraft::Edit(_)) => tr(language, "编辑备忘录", "Edit memo"),
        None => tr(language, "编辑内容", "Edit content"),
    }
}

pub(super) fn save_shortcut_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘S"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+S"
    }
}

pub(super) fn markdown_preview_source(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let mut out = String::with_capacity(text.len() + text.matches('\n').count() * 2);
    let mut in_fence = false;
    let mut lines = text.split('\n').peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        out.push_str(line);
        if lines.peek().is_none() {
            break;
        }
        let paragraph = line.is_empty() || lines.peek().is_some_and(|next| next.is_empty());
        if paragraph || in_fence || fence {
            out.push('\n');
        } else {
            out.push_str("  \n");
        }
        if fence {
            in_fence = !in_fence;
        }
    }
    out
}

pub(super) fn request_standalone_editor_close(
    owner: &WeakEntity<ClipboardView>,
    window: &mut Window,
    cx: &mut App,
) {
    if owner
        .update(cx, |owner, cx| {
            owner.request_editor_close(false, window, cx)
        })
        .unwrap_or(true)
    {
        window.remove_window();
    }
}

impl ClipboardView {
    pub(super) fn lower_main_for_editor(&mut self, window: &Window) {
        if let Some(level) = native_window::lower_for_editor(window) {
            self.editor_main_level = Some(level);
        }
    }

    pub(super) fn restore_main_after_editor(&mut self, cx: &mut Context<Self>) {
        if self.editor_main_level.is_none() {
            return;
        }
        let main = self.main_window;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = owner.update(cx, |view, _| {
                    if view.editor_window.is_none()
                        && let Some(level) = view.editor_main_level.take()
                        && native_window::is_window_shown(window)
                        && !native_window::is_window_minimized(window)
                    {
                        native_window::restore_after_editor(window, level);
                    }
                });
            });
        });
    }

    pub(super) fn open_editor_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        memo_source: Option<SharedString>,
    ) {
        let display_id = window.display(cx).map(|display| display.id());
        let bounds = Bounds::centered(display_id, size(px(EDITOR_WIDTH), px(EDITOR_HEIGHT)), cx);
        let theme = self.theme;
        let language = self.language;
        let memo = self.todo_draft;
        let generation = self.editor.generation;
        let title = editor_title(language, memo);
        let window_effect = self.window_effect.value;
        let main_window = window.window_handle();
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let Some(owner_entity) = owner.upgrade() else {
                return;
            };
            let view = owner_entity.read(cx);
            let current = match memo {
                Some(draft) => view.todo_draft == Some(draft),
                None => view.editor.generation == generation && view.editor_source_hash.is_some(),
            };
            if !current || view.editor_window.is_some() {
                return;
            }
            // Panel is topmost and non-activating; drop that before the editor exists.
            let _ = main_window.update(cx, |_, window, cx| {
                owner_entity.update(cx, |view, _| view.lower_main_for_editor(window));
            });
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    ..TitleBar::title_bar_options()
                }),
                window_min_size: Some(size(px(EDITOR_MIN_WIDTH), px(EDITOR_MIN_HEIGHT))),
                focus: true,
                show: true,
                app_id: Some("com.aslant.ElegantClipboard.editor".into()),
                ..TitleBar::window_options()
            };
            let editor_owner = owner.clone();
            let close_owner = owner.clone();
            let opened = gpui_kit::open_window(options, cx, move |window, cx| {
                apply_theme(theme, window, cx);
                acrylic::init_window_effect(window_effect, window, cx);
                window.set_window_title(title);
                window.on_window_should_close(cx, move |window, cx| {
                    request_standalone_editor_close(&close_owner, window, cx);
                    false
                });
                cx.new(|cx| {
                    content_editor::ContentEditorWindow::new(editor_owner, memo_source, window, cx)
                })
            });
            let handle = owner_entity.update(cx, |owner, cx| {
                let handle = match opened {
                    Ok((handle, _)) => {
                        owner.editor_window = Some(handle);
                        Some(handle)
                    }
                    Err(error) => {
                        owner.fail_content_editor_open(error, cx);
                        owner.restore_main_after_editor(cx);
                        None
                    }
                };
                cx.notify();
                handle
            });
            if let Some(handle) = handle {
                let _ = handle.update(cx, |_, editor, _| {
                    native_window::raise_front(editor);
                    native_window::activate_for_input(editor);
                });
            }
        });
    }

    pub(super) fn close_content_editor_from_window(&mut self, cx: &mut Context<Self>) -> bool {
        if self.editor_pending() || self.todo_edit_saving {
            return false;
        }
        let focus = self.list_focus.clone();
        let main = self.main_window;
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                if !window.has_active_dialog(cx) {
                    window.focus(&focus, cx);
                }
            });
        });
        cx.notify();
        true
    }

    fn fail_content_editor_open(&mut self, error: impl std::fmt::Debug, cx: &mut Context<Self>) {
        self.editor_window = None;
        self.release_editor_session();
        self.set_feedback_message(format!(
            "{}: {error:?}",
            tr(
                self.language,
                "无法打开编辑窗口",
                "Unable to open the editor"
            )
        ));
        self.is_error = true;
        cx.notify();
    }
}
