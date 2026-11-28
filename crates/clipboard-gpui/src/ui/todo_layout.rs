use super::*;

#[derive(Default)]
pub(super) struct TodoListLayout {
    pub ids: Vec<i64>,
    pub sources: Vec<usize>,
    pub indexes: HashMap<i64, usize>,
    pub sizes: Rc<Vec<gpui_kit::Size<Pixels>>>,
    pub offsets: Vec<Pixels>,
    row_heights: HashMap<i64, Pixels>,
    heights_dirty: bool,
    expanded_row: Option<i64>,
    filter_dirty: bool,
    query: String,
    show_completed: bool,
    key: Option<LayoutKey>,
}

struct LayoutKey {
    width: Pixels,
    rem: Pixels,
    font_family: String,
    card: MemoCardLayout,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct MemoCardLayout {
    pub(super) density: CardDensity,
    pub(super) font_size: u8,
    max_card_height: f32,
}

impl MemoCardLayout {
    pub(super) fn max_row_height(self) -> Pixels {
        px(self.max_card_height + 2. * visual::card_spacing(self.density))
    }

    pub(super) fn estimated_row_height(self) -> Pixels {
        px(visual::row_height_for_body(
            self.density,
            f32::from(self.font_size) * visual::COPY_LINE_HEIGHT,
        ))
    }

    fn expanded_footer_height(self) -> Pixels {
        px(visual::CONTROL_HEIGHT_COMPACT + visual::card_spacing(self.density))
    }

    pub(super) fn body_height(self, row_height: Pixels, expanded: bool) -> Pixels {
        let chrome = px(visual::row_height_for_body(self.density, 0.))
            + if expanded {
                self.expanded_footer_height()
            } else {
                px(0.)
            };
        (row_height - chrome).max(px(0.))
    }
}

impl TodoListLayout {
    pub fn invalidate_filter(&mut self) {
        self.filter_dirty = true;
    }

    pub fn has_current_filter(&self) -> bool {
        !self.filter_dirty
    }

    pub(super) fn record_row_height(&mut self, id: i64, height: Pixels) -> bool {
        if height <= px(0.) || self.row_heights.get(&id) == Some(&height) {
            return false;
        }
        self.row_heights.insert(id, height);
        // Rebuild once after all visible rows have reported their bounds.
        !std::mem::replace(&mut self.heights_dirty, true)
    }

    pub(super) fn retain_row_heights(&mut self, before: &[Todo], after: &[Todo]) {
        if !self.row_heights.is_empty() {
            let next: HashMap<_, _> = after
                .iter()
                .map(|todo| (todo.id, todo.text.as_str()))
                .collect();
            for todo in before {
                if next.get(&todo.id).copied() != Some(todo.text.as_str()) {
                    self.row_heights.remove(&todo.id);
                }
            }
        }
        self.invalidate_filter();
    }

    pub(super) fn row_height(&self, id: i64, card: MemoCardLayout, expanded: bool) -> Pixels {
        let limit = card.max_row_height();
        if expanded {
            self.row_heights.get(&id).map_or(limit, |height| {
                (*height + card.expanded_footer_height()).min(limit)
            })
        } else {
            self.row_heights
                .get(&id)
                .copied()
                .unwrap_or_else(|| card.estimated_row_height())
        }
    }

    fn sync_filter(&mut self, todos: &[Todo], query: &str, show_completed: bool) {
        self.query.clear();
        self.query.push_str(query);
        self.show_completed = show_completed;
        let normalized = query.to_lowercase();
        self.ids.clear();
        self.sources.clear();
        self.indexes.clear();
        for (source, todo) in todos.iter().enumerate() {
            if (show_completed || !todo.status.is_finished())
                && (normalized.is_empty() || todo.text.to_lowercase().contains(&normalized))
            {
                self.indexes.insert(todo.id, self.ids.len());
                self.ids.push(todo.id);
                self.sources.push(source);
            }
        }
        self.filter_dirty = false;
    }

    pub(super) fn selected_index(&self, id: Option<i64>) -> Option<usize> {
        if self.filter_dirty {
            return None;
        }
        id.and_then(|id| self.indexes.get(&id).copied())
    }
}

impl ClipboardView {
    pub(super) fn memo_card_layout(&self) -> MemoCardLayout {
        let history_density = self.display.value.card_density;
        let font_size = self.fonts.value.card_font_size;
        let line_height = f32::from(font_size) * visual::COPY_LINE_HEIGHT;
        let history_card = visual::row_height_for_body(history_density, line_height)
            - 2. * visual::card_spacing(history_density);
        MemoCardLayout {
            density: self.todo_preference.value.density(history_density),
            font_size,
            max_card_height: history_card * 4.,
        }
    }

    /// Sole owner of filtered indexes and the geometry used by render and dragging.
    pub(super) fn sync_todo_layout(&mut self, window: &Window, cx: &App) {
        let mut layout = self.todo_layout.borrow_mut();
        let query = self.search.read(cx).value();
        let query = query.trim();
        let preference = self.todo_preference.value;
        let width = window.viewport_size().width;
        let rem = window.rem_size();
        let card = self.memo_card_layout();
        let fonts = &self.fonts.value;
        let family = if fonts.card_font.is_empty() {
            &fonts.ui_font
        } else {
            &fonts.card_font
        };
        let style_changed = layout.key.as_ref().is_none_or(|key| {
            key.width != width || key.rem != rem || key.card != card || &key.font_family != family
        });
        let projection_changed =
            layout.query != query || layout.show_completed != preference.show_completed;
        let expansion_changed = layout.expanded_row != self.memo_preview;
        let filter_changed = layout.filter_dirty || projection_changed;
        if !filter_changed && !expansion_changed && !style_changed && !layout.heights_dirty {
            return;
        }
        let scroll_anchor = ((!filter_changed || (expansion_changed && !projection_changed))
            && !layout.offsets.is_empty())
        .then(|| {
            let (index, partial) = todo_scroll_anchor(&layout.offsets, self.todo_scroll.offset().y);
            (layout.ids.get(index).copied(), index, partial)
        });
        if style_changed {
            layout.row_heights.clear();
            layout.key = Some(LayoutKey {
                width,
                rem,
                font_family: family.clone(),
                card,
            });
        }
        if filter_changed {
            layout.sync_filter(&self.todos, query, preference.show_completed);
            if self
                .memo_preview
                .is_some_and(|id| !layout.indexes.contains_key(&id))
            {
                self.memo_preview = None;
            }
            if layout.selected_index(self.selected_todo).is_none() {
                self.selected_todo = None;
            }
        }
        layout.expanded_row = self.memo_preview;
        let mut sizes = Vec::with_capacity(layout.ids.len() + 1);
        let mut offsets = Vec::with_capacity(layout.ids.len() + 1);
        let mut top = px(0.);
        offsets.push(top);
        for &id in &layout.ids {
            let height = layout.row_height(id, card, layout.expanded_row == Some(id));
            sizes.push(size(width, height));
            top += height;
            offsets.push(top);
        }
        // A non-card footer preserves clearance without extending a draggable row.
        sizes.push(size(
            width,
            px(visual::CONTROL_HEIGHT_LARGE + visual::SPACE_6),
        ));
        layout.sizes = Rc::new(sizes);
        layout.heights_dirty = false;
        layout.offsets = offsets;
        if let Some((id, index, partial)) = scroll_anchor {
            let index = id
                .and_then(|id| layout.indexes.get(&id).copied())
                .unwrap_or_else(|| index.min(layout.ids.len()));
            let top = layout.offsets[index];
            let partial = partial.min(layout.sizes[index].height);
            self.todo_scroll.set_offset(point(px(0.), -(top + partial)));
        }
        drop(layout);
        if filter_changed || style_changed {
            self.drag_motion = DragMotionState::default();
            self.todo_drop_target = None;
        }
        if projection_changed || style_changed {
            self.todo_move_offsets.clear();
        }
    }
}

fn todo_scroll_anchor(offsets: &[Pixels], scroll_y: Pixels) -> (usize, Pixels) {
    let top = -scroll_y;
    let index = offsets
        .partition_point(|offset| *offset <= top)
        .saturating_sub(1);
    (index, top - offsets[index])
}

#[cfg(test)]
mod tests {
    use super::{MemoKind, Todo, TodoListLayout, TodoStatus, px, todo_scroll_anchor};
    #[test]
    fn scroll_anchor_preserves_partial_rows_and_footer_clearance() {
        let offsets = [px(0.), px(64.), px(172.)];
        assert_eq!(todo_scroll_anchor(&offsets, px(-93.)), (1, px(29.)));
        assert_eq!(todo_scroll_anchor(&offsets, px(-190.)), (2, px(18.)));
    }

    #[test]
    fn memo_selection_only_survives_when_the_same_record_is_visible() {
        let todo = |id, text: &str, status| Todo {
            id,
            text: text.into(),
            status,
            kind: MemoKind::Todo,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let mut layout = TodoListLayout::default();
        let mut rows = [
            todo(1, "Alpha", TodoStatus::Pending),
            todo(2, "alpha also", TodoStatus::Done),
        ];
        layout.sync_filter(&rows, "ALPHA", true);
        assert_eq!(layout.selected_index(Some(1)), Some(0));
        assert_eq!(layout.selected_index(Some(2)), Some(1));

        // Saving text out of the search while another memo still matches
        // must not leave the old ID actionable or transfer its selection.
        rows[0].text = "no longer matches".into();
        layout.invalidate_filter();
        assert_eq!(layout.selected_index(Some(1)), None);
        layout.sync_filter(&rows, "ALPHA", true);
        assert_eq!(layout.selected_index(Some(1)), None);
        assert_eq!(layout.selected_index(None), None);
        assert_eq!(layout.selected_index(Some(2)), Some(0));

        layout.sync_filter(&rows, "ALPHA", false);
        assert_eq!(layout.selected_index(Some(2)), None);
        layout.sync_filter(&rows[..1], "", true);
        assert_eq!(layout.selected_index(Some(2)), None);
    }
}
