use super::todo_notice::TodoNotice;
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TodoDraft {
    New(MemoKind),
    Edit(i64),
}

impl ClipboardView {
    pub(super) fn todo_editor_unavailable(&self) -> bool {
        self.editor_pending()
            || self.todo_edit_saving
            || self.editor.id.is_some()
            || self.editor_window.is_some()
            || self.todo_draft.is_some()
            || matches!(self.update_check, UpdateCheck::Installing)
    }

    pub(super) fn open_todo_editor(
        &mut self,
        id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.todos_open
            || self.todos_pending
            || self.card_drag_active(cx)
            || window.has_active_dialog(cx)
        {
            return;
        }
        if self.todo_editor_unavailable() {
            if let Some(message) = self.editor_draft_blocker(cx) {
                self.reveal_editor(message, cx);
            }
            return;
        }
        let (draft, source) = if let Some(id) = id {
            self.sync_todo_layout(window, cx);
            let layout = self.todo_layout.borrow();
            let Some(index) = layout.selected_index(Some(id)) else {
                return;
            };
            let source = self.todos[layout.sources[index]].text.as_str().into();
            self.selected_todo = Some(id);
            (TodoDraft::Edit(id), source)
        } else {
            (TodoDraft::New(MemoKind::Todo), SharedString::default())
        };
        self.close_history_menu(cx);
        self.todo_draft = Some(draft);
        self.editor_error = None;
        self.open_editor_window(window, cx, Some(source));
        cx.notify();
    }

    pub(super) fn save_todo_editor(&mut self, text: String, cx: &mut Context<Self>) {
        if self.todos_pending || self.editor_discard_open {
            return;
        }
        let Some(draft) = self.todo_draft else {
            return;
        };
        let command = match draft {
            TodoDraft::New(kind) => Command::AddTodo { text, kind },
            TodoDraft::Edit(id) => Command::UpdateTodo { id, text },
        };
        if self.send(command, cx) {
            if matches!(draft, TodoDraft::New(_)) {
                self.todo_notice = Some(TodoNotice::Added);
            }
            self.todos_pending = true;
            self.todo_edit_saving = true;
            self.editor_error = None;
        } else {
            self.editor_error = Some(self.message.clone());
        }
        cx.notify();
    }
}

pub(super) fn memo_kind_toggle(
    kind: MemoKind,
    language: LanguagePreference,
    owner: WeakEntity<ClipboardView>,
    pending: bool,
) -> TabBar {
    TabBar::new("memo-kind")
        .segmented()
        .h_8()
        .w(rems(10.5))
        .selected_index(usize::from(kind == MemoKind::Note))
        .children(
            [tr(language, "待办", "To-do"), tr(language, "笔记", "Note")].map(|label| {
                Tab::new()
                    .label(label)
                    .aria_label(label)
                    .disabled(pending)
                    .text_sm()
                    .flex_1()
            }),
        )
        .on_click(move |index: &usize, _, cx| {
            let next = if *index == 0 {
                MemoKind::Todo
            } else {
                MemoKind::Note
            };
            let _ = owner.update(cx, |view, cx| {
                if view.todo_edit_saving || view.editor_discard_open {
                    return;
                }
                if let Some(TodoDraft::New(kind)) = &mut view.todo_draft
                    && *kind != next
                {
                    *kind = next;
                    cx.notify();
                }
            });
        })
}
