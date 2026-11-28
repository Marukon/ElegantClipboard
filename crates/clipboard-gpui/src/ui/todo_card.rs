use super::todo_drag::TodoDrag;
use super::todo_layout::MemoCardLayout;
use super::*;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::text::TextView;

#[derive(Clone)]
pub(super) struct TodoDragSnapshot {
    pub(super) text: SharedString,
    pub(super) layout: MemoCardLayout,
    summary: SharedString,
}

impl TodoDragSnapshot {
    pub(super) fn surface(
        &self,
        id: i64,
        expanded_body: Option<Pixels>,
        selected: bool,
        dragging: bool,
        cx: &App,
    ) -> Div {
        let layout = self.layout;
        let expanded = expanded_body.is_some();
        let font_size = f32::from(layout.font_size);
        let text = TextView::markdown(("memo-document", id as usize), self.text.clone())
            .font_family(crate::fonts::resolve_card_family(
                &crate::design_system::active_fonts(cx),
            ))
            .text_size(px(font_size))
            .line_height(relative(visual::COPY_LINE_HEIGHT))
            .selectable(expanded)
            .scrollable(expanded);
        let text = if expanded {
            text
        } else {
            let cap = layout.body_height(layout.max_row_height(), false);
            let lines = (f32::from(cap) / (font_size * visual::COPY_LINE_HEIGHT)).ceil() as usize;
            // Card clicks expand the memo rather than following embedded links.
            text.max_lines(lines).max_h(cap).link(|_, _| None)
        };
        div()
            .relative()
            .w_full()
            .p(px(visual::card_padding(layout.density)))
            .gap(px(visual::card_spacing(layout.density)))
            .flex()
            .flex_col()
            .rounded(crate::card_radius::card_radius(cx))
            .overflow_hidden()
            .border(px(visual::BORDER_WIDTH))
            .border_color(if selected {
                cx.theme().list_active_border
            } else {
                crate::design_system::container_border(cx)
            })
            .bg(if dragging {
                cx.theme().popover
            } else if selected {
                cx.theme().list_active
            } else {
                cx.theme().colors.list
            })
            .child(
                div()
                    .when_some(expanded_body, |body, height| body.h(height))
                    .flex_none()
                    .min_w_0()
                    .overflow_hidden()
                    .child(text),
            )
            .child(
                div()
                    .h(px(visual::METADATA_ROW_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .text_size(px(visual::TEXT_SMALL))
                    .text_color(cx.theme().muted_foreground.opacity(0.85))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(self.summary.clone()),
                    ),
            )
    }
}

struct MemoSource {
    revision: u64,
    text: SharedString,
}

pub(super) fn todo_empty(message: &str, cx: &App) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .px(px(visual::SPACE_6))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(visual::SPACE_3))
        .child(
            div()
                .size(px(48.))
                .rounded(cx.theme().radius_lg)
                .bg(cx.theme().muted)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(IconName::CircleCheck)
                        .size(px(visual::SPACE_6))
                        .text_color(cx.theme().muted_foreground),
                ),
        )
        .child(
            div()
                .max_w(px(320.))
                .text_size(px(visual::TEXT_BODY))
                .text_center()
                .line_height(relative(1.5))
                .text_color(cx.theme().muted_foreground)
                .child(message.to_owned()),
        )
}

pub(super) fn card_excerpt(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            line.trim_start_matches(|ch: char| "#-*`> ".contains(ch))
                .replace("**", "")
                .replace("__", "")
        })
        .filter(|line| !line.trim().is_empty())
        .unwrap_or_default()
}

impl ClipboardView {
    pub(super) fn close_memo_preview(&mut self, cx: &mut Context<Self>) -> bool {
        if self.memo_preview.take().is_none() {
            return false;
        }
        cx.notify();
        true
    }

    fn open_memo_preview(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.todos_open || self.card_drag_active(cx) {
            return;
        }
        self.sync_todo_layout(window, cx);
        let Some(index) = self.todo_layout.borrow().selected_index(Some(id)) else {
            return;
        };
        self.memo_preview = Some(id);
        self.selected_todo = Some(id);
        self.todo_move_offsets.clear();
        self.todo_scroll
            .scroll_to_item(index, ScrollStrategy::Nearest);
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    fn render_memo_preview_actions(&self, id: i64, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        div()
            .h(px(visual::CONTROL_HEIGHT_COMPACT))
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("memo-preview-collapse")
                    .ghost()
                    .small()
                    .label(tr(language, "收起", "Collapse"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.close_memo_preview(cx);
                        window.focus(&view.list_focus, cx);
                    })),
            )
            .child(
                Button::new("memo-preview-edit")
                    .outline()
                    .small()
                    .label(tr(language, "编辑", "Edit"))
                    .disabled(self.todos_pending || self.todo_editor_unavailable())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.open_todo_editor(Some(id), window, cx);
                    })),
            )
    }

    pub(super) fn render_todo_item(
        &self,
        index: usize,
        card_layout: MemoCardLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let layout = self.todo_layout.borrow();
        let Some(&source) = layout.sources.get(index) else {
            return div()
                .h(px(visual::CONTROL_HEIGHT_LARGE + visual::SPACE_6))
                .into_any_element();
        };
        let todo = &self.todos[source];
        let id = todo.id;
        let row_height = layout.sizes[index].height;
        let (from, to) = self.drag_motion.offsets(index, &layout.offsets);
        drop(layout);

        let revision = self.todo_revision;
        let source = window.use_keyed_state(("memo-source", id as usize), cx, |_, _| MemoSource {
            revision,
            text: todo.text.as_str().into(),
        });
        if source.read(cx).revision != revision {
            source.update(cx, |source, _| {
                source.revision = revision;
                if source.text.as_ref() != todo.text.as_str() {
                    source.text = todo.text.as_str().into();
                }
            });
        }
        let snapshot = TodoDragSnapshot {
            text: source.read(cx).text.clone(),
            layout: card_layout,
            summary: todo_summary(self, todo),
        };
        if self.memo_preview == Some(id) {
            let body_height = card_layout.body_height(row_height, true);
            return Self::todo_row_frame(id, card_layout, row_height)
                .child(
                    snapshot
                        .surface(
                            id,
                            Some(body_height),
                            self.selected_todo == Some(id),
                            false,
                            cx,
                        )
                        .child(self.render_memo_preview_actions(id, cx)),
                )
                .into_any_element();
        }
        let row = self.render_todo_row(todo, snapshot, row_height, cx);
        if from != 0. || to != 0. {
            visual::drag_yield(
                div().child(row),
                from,
                to,
                self.drag_motion.started,
                self.drag_motion.progress.clone(),
                cx,
            )
        } else if let Some(offset) = self.todo_move_offsets.get(&id).copied() {
            visual::reflow(
                div().child(row),
                offset,
                format!("todo-move-{}-{id}", self.todo_motion_revision),
                cx,
            )
        } else {
            row
        }
    }

    fn render_todo_row(
        &self,
        todo: &Todo,
        snapshot: TodoDragSnapshot,
        row_height: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = todo.id;
        let status = todo.status;
        let text = snapshot.text.clone();
        let layout = snapshot.layout;
        let pending = self.todos_pending;
        let entity = cx.entity();
        let menu_owner = entity.clone();
        let card_padding = visual::card_padding(layout.density);
        let measured_size = Rc::new(Cell::new(size(px(0.), px(0.))));
        let surface = snapshot
            .surface(id, None, self.selected_todo == Some(id), false, cx)
            .group("todo-card")
            .child(todo_action_bar(self, id, pending, card_padding, cx));
        let drag = TodoDrag {
            id,
            snapshot,
            size: measured_size.clone(),
            native_preview: false,
        };
        let drag_enabled = !pending && !self.todo_reorder_pending && self.todos_open;
        let measured_layout = self.todo_layout.clone();
        let list = self.list_surface.downgrade();
        Self::todo_row_frame(id, layout, row_height)
            .child(
                div()
                    .relative()
                    .flex_none()
                    .w_full()
                    .child(surface)
                    .on_prepaint(move |bounds, window, _| {
                        let height = (bounds.size.height
                            + px(2. * visual::card_spacing(layout.density)))
                        .min(layout.max_row_height());
                        measured_size.set(size(bounds.size.width + px(2. * PAGE_PADDING), height));
                        if measured_layout.borrow_mut().record_row_height(id, height) {
                            window.on_next_frame(move |_, cx| {
                                let _ = list.update(cx, |_, cx| cx.notify());
                            });
                        }
                    }),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && !this.card_drag_active(cx) {
                    this.select_todo(id, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.prepare_memo_drag(id, &text, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                move |event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ClipboardView::open_todo_menu(
                        menu_owner.clone(),
                        id,
                        status,
                        event.position,
                        window,
                        cx,
                    );
                },
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_memo_preview(id, window, cx);
            }))
            .when(drag_enabled, |row| {
                row.cursor_grab()
                    .on_drag(drag, move |drag, offset, window, cx| {
                        let mut preview = drag.clone();
                        entity.update(cx, |this, cx| {
                            this.close_memo_preview(cx);
                            this.todo_drop_target = None;
                            this.todo_move_offsets.clear();
                            this.drag_motion.hovered = None;
                            this.start_card_drag_scroll(cx);
                            preview.native_preview = this.begin_card_drag(
                                external_drag::CardDrag::Todo(Rc::new(drag.clone())),
                                offset,
                                window,
                                cx,
                            );
                            cx.notify();
                        });
                        cx.new(|_| preview)
                    })
            })
            .when(self.todo_drag_source_id() == Some(id), |row| {
                row.opacity(0.)
            })
            .into_any_element()
    }

    fn todo_row_frame(id: i64, layout: MemoCardLayout, height: Pixels) -> Stateful<Div> {
        div()
            .id(("todo-row", id as usize))
            .relative()
            .w_full()
            .h(height)
            .flex()
            .flex_col()
            // Keep a new estimate from letting a tall card paint over its neighbor.
            .overflow_hidden()
            .px(px(PAGE_PADDING))
            .py(px(visual::card_spacing(layout.density)))
    }
}

fn todo_action_bar(
    view: &ClipboardView,
    id: i64,
    pending: bool,
    card_padding: f32,
    cx: &mut Context<ClipboardView>,
) -> impl IntoElement {
    div()
        .id(("todo-actions", id as usize))
        .absolute()
        .top(px(card_padding))
        .right(px(card_padding))
        .h(px(visual::CONTROL_HEIGHT_COMPACT))
        .px(px(visual::SPACE_1 / 2.))
        .flex()
        .items_center()
        .gap(px(visual::SPACE_1))
        .rounded(crate::card_radius::card_radius(cx))
        .overflow_hidden()
        .bg(cx.theme().popover)
        .text_color(cx.theme().popover_foreground)
        .border(px(visual::BORDER_WIDTH))
        .border_color(cx.theme().border)
        .shadow(visual::control_shadow())
        .invisible()
        .when(!view.card_drag_active(cx), |bar| {
            bar.group_hover("todo-card", |style| style.visible())
        })
        .child(
            todo_icon_button(
                ("todo-edit", id as usize),
                IconName::FileText,
                tr(view.language, "编辑", "Edit"),
                pending || view.todo_editor_unavailable(),
            )
            .on_click(cx.listener(move |view, _, window, cx| {
                cx.stop_propagation();
                view.open_todo_editor(Some(id), window, cx);
            })),
        )
        .child(
            todo_icon_button(
                ("todo-remove", id as usize),
                gpui_kit::assets::IconName::Trash,
                tr(view.language, "移除", "Remove"),
                pending,
            )
            .on_click(cx.listener(move |view, _, _, cx| {
                cx.stop_propagation();
                view.remove_todo(id, cx);
            })),
        )
}

fn todo_icon_button(
    id: impl Into<ElementId>,
    icon: impl Into<Icon>,
    label: impl Into<SharedString>,
    disabled: bool,
) -> Button {
    let label = label.into();
    Button::new(id)
        .ghost()
        .xsmall()
        .h(px(visual::SPACE_6))
        .icon(icon.into().size(px(visual::ICON_SMALL)))
        .tooltip(label.clone())
        .accessibility_label(label)
        .disabled(disabled)
}

fn todo_summary(view: &ClipboardView, todo: &Todo) -> SharedString {
    let language = view.language;
    let mut summary = match todo.kind {
        MemoKind::Note => tr(language, "笔记", "Note"),
        MemoKind::Todo => tr(language, "待办", "To-do"),
    }
    .to_owned();
    if todo.status != TodoStatus::Pending {
        summary.push_str(" · ");
        summary.push_str(super::todo_notice::todo_status_label(language, todo.status));
    }
    let preference = view.todo_preference.value;
    if preference.show_created || preference.show_edited {
        let now = chrono::Local::now().naive_local();
        for (show, timestamp, label) in [
            (
                preference.show_created,
                &todo.created_at,
                tr(language, "创建 ", "Created "),
            ),
            (
                preference.show_edited,
                &todo.updated_at,
                tr(language, "编辑 ", "Edited "),
            ),
        ] {
            if show {
                summary.push_str(" · ");
                summary.push_str(label);
                summary.push_str(&format_card_time(
                    timestamp,
                    view.display.value.time_format,
                    language,
                    now,
                ));
            }
        }
    }
    summary.into()
}
