//! Searchable Combobox & Select Dropdown Component
//!
//! Reusable searchable combobox / select dropdown based on Cydonia & Bezel architecture:
//! - Pure ranked filtering (`Filter`, `match_rank`, `filter_indices`):
//!   Prefix matches are ranked first (rank 0), substring matches second (rank 1).
//! - Active keyboard navigation (`menu_step`, Up, Down, Enter, Escape).
//! - Trigger press capture safety (`ComboboxState` with `note_press` and `take_press_was_open`):
//!   Eliminates outside-click vs trigger-click race conditions.
//! - Search row (`search_line`) with dedicated search icon and borderless input.
//! - Selection indicator (Checkmark `✓`).

use gpui::{
    Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, Window, deferred, div,
    prelude::*, px,
};
use std::rc::Rc;

use crate::text_input::{TextInput, TextInputEvent};
use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Pure Reducer: match_rank, filter_indices, menu_step, Filter
// ---------------------------------------------------------------------------

/// Match rank of a label against a query:
/// - `Some(0)`: Prefix match (`label.starts_with(query)`).
/// - `Some(1)`: Substring match (`label.contains(query)`).
/// - `None`: No match.
/// Case-insensitive. An empty query matches all with rank 1 (preserving original order).
pub fn match_rank(query: &str, label: &str) -> Option<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(1);
    }
    let label = label.to_lowercase();
    if label.starts_with(&query) {
        Some(0)
    } else if label.contains(&query) {
        Some(1)
    } else {
        None
    }
}

/// Filter + rank labels for a search query: prefix matches first, then
/// substring matches, stable within each rank. Returns indices into `labels`.
pub fn filter_indices<S: AsRef<str>>(query: &str, labels: &[S]) -> Vec<usize> {
    let mut ranked: Vec<(usize, usize)> = labels
        .iter()
        .enumerate()
        .filter_map(|(ix, label)| match_rank(query, label.as_ref()).map(|rank| (rank, ix)))
        .collect();
    ranked.sort_by_key(|&(rank, ix)| (rank, ix));
    ranked.into_iter().map(|(_, ix)| ix).collect()
}

/// Match rank of a model against a search query:
/// - Rank 0: model ID starts with query
/// - Rank 1: model ID contains query
/// - Rank 2: display name starts with query
/// - Rank 3: display name contains query
/// - Rank 4: vendor / owned_by contains query
/// - None: no match
///
/// Case-insensitive. An empty query matches all with rank 1 (preserving original order).
pub fn match_model_rank(
    query: &str,
    model_id: &str,
    display_name: Option<&str>,
    vendor: Option<&str>,
) -> Option<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(1);
    }
    let id_lower = model_id.to_lowercase();
    if id_lower.starts_with(&query) {
        return Some(0);
    }
    if id_lower.contains(&query) {
        return Some(1);
    }
    if let Some(dn) = display_name {
        let dn_lower = dn.to_lowercase();
        if dn_lower.starts_with(&query) {
            return Some(2);
        }
        if dn_lower.contains(&query) {
            return Some(3);
        }
    }
    if let Some(v) = vendor {
        let v_lower = v.to_lowercase();
        if v_lower.starts_with(&query) || v_lower.contains(&query) {
            return Some(4);
        }
    }
    None
}

/// Filter + rank fetched models for a search query using [`match_model_rank`].
/// Returns references to matched models in ranked order, stable within each rank.
pub fn filter_models<'a>(
    query: &str,
    models: &'a [aitoolplus_core::api_hub::FetchedModel],
) -> Vec<&'a aitoolplus_core::api_hub::FetchedModel> {
    let mut ranked: Vec<(usize, usize, &'a aitoolplus_core::api_hub::FetchedModel)> = models
        .iter()
        .enumerate()
        .filter_map(|(orig_ix, m)| {
            match_model_rank(
                query,
                &m.id,
                m.display_name.as_deref(),
                m.owned_by.as_deref(),
            )
            .map(|rank| (rank, orig_ix, m))
        })
        .collect();
    ranked.sort_by_key(|&(rank, orig_ix, _)| (rank, orig_ix));
    ranked.into_iter().map(|(_, _, m)| m).collect()
}

/// Step the active row of a menu: wraps at both ends.
/// Empty menus return `None`.
pub fn menu_step(active: Option<usize>, count: usize, delta: isize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let count_i = count as isize;
    let next = match active {
        None => {
            if delta >= 0 {
                0
            } else {
                count_i - 1
            }
        }
        Some(at) => (at as isize + delta).rem_euclid(count_i),
    };
    Some(next as usize)
}

/// Filter state behind a searchable picker, directly mirroring Bezel's `Filter`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    items: Vec<SharedString>,
    /// Indices into `items`, ranked by [`filter_indices`].
    filtered: Vec<usize>,
    /// Position within `filtered`, not within `items`.
    active: Option<usize>,
}

impl Filter {
    pub fn new(items: Vec<SharedString>) -> Self {
        let filtered: Vec<usize> = (0..items.len()).collect();
        let active = (!filtered.is_empty()).then_some(0);
        Self {
            items,
            filtered,
            active,
        }
    }

    pub fn items(&self) -> &[SharedString] {
        &self.items
    }

    pub fn filtered(&self) -> &[usize] {
        &self.filtered
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    /// Re-rank against `query`, re-entering the list at the top.
    pub fn refilter(&mut self, query: &str) {
        self.filtered = filter_indices(query, &self.items);
        self.active = (!self.filtered.is_empty()).then_some(0);
    }

    pub fn step(&mut self, delta: isize) {
        self.active = menu_step(self.active, self.filtered.len(), delta);
    }

    pub fn set_active(&mut self, position: usize) {
        if position < self.filtered.len() {
            self.active = Some(position);
        }
    }

    /// The item index confirming right now would pick (an index into `items`, NOT `filtered`).
    pub fn active_item(&self) -> Option<usize> {
        self.active
            .and_then(|position| self.filtered.get(position))
            .copied()
    }

    pub fn active_value(&self) -> Option<&SharedString> {
        self.active_item().and_then(|idx| self.items.get(idx))
    }
}

// ---------------------------------------------------------------------------
// Combobox State & Lifecycle Safety
// ---------------------------------------------------------------------------

/// State tracker for dropdown popup toggle and click-out race protection.
///
/// Implements Bezel's `pressed_while_open` pattern:
/// A capture-phase listener on the trigger notes whether the popup was already
/// open at the moment of `mouse_down`. When the subsequent click/release occurs,
/// `take_press_was_open()` checks whether this press originated while open. If so,
/// the click leaves it closed instead of re-opening it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComboboxState {
    pub is_open: bool,
    pub pressed_while_open: bool,
    pub active_index: Option<usize>,
}

impl ComboboxState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Note whether the popup was open when the trigger was pressed down.
    pub fn note_press(&mut self) {
        self.pressed_while_open = self.is_open;
    }

    /// Consume the press note. Returns `true` if the press occurred while open.
    pub fn take_press_was_open(&mut self) -> bool {
        std::mem::take(&mut self.pressed_while_open)
    }

    pub fn open(&mut self) {
        self.is_open = true;
        self.active_index = Some(0);
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.active_index = None;
    }

    /// Toggle dropdown open/closed state safely. Returns the new `is_open` status.
    pub fn toggle(&mut self) -> bool {
        if self.take_press_was_open() {
            self.close();
            false
        } else {
            self.open();
            true
        }
    }
}

// ---------------------------------------------------------------------------
// Reusable Searchable Combobox Component
// ---------------------------------------------------------------------------

/// Renders a modern searchable dropdown / combobox component matching Cydonia/Bezel design.
///
/// Features:
/// - Styled closed face displaying the current value, placeholder, clear button, and chevron.
/// - Outside click & trigger click race protection.
/// - Anchored floating popup using `gpui::deferred` to prevent layout jumps.
/// - Unframed search input row (`search_line`) with Lucide magnifying glass icon.
/// - Real-time ranked prefix/substring filtering.
/// - Active row keyboard highlight and mouse hover synchronization.
/// - Selection checkmark (`✓`).
pub fn searchable_combobox<V: 'static>(
    id: impl Into<SharedString>,
    label: Option<impl Into<SharedString>>,
    current_value: Option<&str>,
    placeholder: impl Into<SharedString>,
    search_input: &gpui::Entity<TextInput>,
    is_open: bool,
    options: &[String],
    active_index: Option<usize>,
    empty_hint: Option<impl Into<SharedString>>,
    theme: &Theme,
    cx: &mut Context<V>,
    on_toggle: impl Fn(&mut V, bool, &mut Window, &mut Context<V>) + 'static,
    on_note_press: impl Fn(&mut V, &mut Context<V>) + 'static,
    on_select: impl Fn(&mut V, String, &mut Context<V>) + 'static,
    on_clear: Option<impl Fn(&mut V, &mut Context<V>) + 'static>,
    on_set_active: Option<impl Fn(&mut V, usize, &mut Context<V>) + 'static>,
) -> gpui::AnyElement {
    let t = theme.clone();
    let id_str: SharedString = id.into();
    let placeholder_str: SharedString = placeholder.into();
    let empty_hint_str: SharedString = empty_hint
        .map(Into::into)
        .unwrap_or_else(|| SharedString::from("无匹配项"));

    let has_val = current_value.is_some_and(|v| !v.trim().is_empty());
    let display_text = current_value
        .filter(|v| !v.trim().is_empty())
        .map(SharedString::from);

    let on_toggle = Rc::new(on_toggle);
    let on_note_press = Rc::new(on_note_press);
    let on_select = Rc::new(on_select);
    let on_clear = on_clear.map(Rc::new);
    let on_set_active = on_set_active.map(Rc::new);

    let mut col = div()
        .id(SharedString::from(format!("{id_str}-col")))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .flex_1()
        .min_w(px(0.0))
        .relative();

    // Optional top label
    let has_label = label.is_some();
    if let Some(lbl) = label {
        col = col.child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .child(lbl.into()),
        );
    }

    // Trigger face
    let on_toggle_click = on_toggle.clone();
    let on_note_press_cap = on_note_press.clone();
    let mut trigger_box = div()
        .id(id_str.clone())
        .h(px(34.0))
        .w_full()
        .rounded(px(6.0))
        .border_1()
        .border_color(if is_open { t.accent } else { t.input_border })
        .bg(t.input_bg)
        .flex()
        .items_center()
        .justify_between()
        .px(px(10.0))
        .cursor_pointer()
        .hover(|h| h.border_color(if is_open { t.accent } else { t.card_border_hover }))
        .capture_any_mouse_down(cx.listener(move |view, _ev: &gpui::MouseDownEvent, _window, cx| {
            on_note_press_cap(view, cx);
        }))
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                cx.stop_propagation();
                on_toggle_click(view, !is_open, window, cx);
            }),
        );

    // Trigger value or placeholder
    let label_view = if let Some(val) = display_text {
        div()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .text_size(px(13.0))
            .text_color(t.text_primary)
            .child(val)
    } else {
        div()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .text_size(px(13.0))
            .text_color(t.text_muted)
            .child(placeholder_str)
    };

    // Right-side icons: clear button (if value present) + chevron toggle
    let mut right_icons = div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .flex_none();

    if has_val {
        if let Some(on_clear_fn) = on_clear {
            let on_clear_click = on_clear_fn.clone();
            right_icons = right_icons.child(
                div()
                    .id(SharedString::from(format!("{id_str}-clear-btn")))
                    .cursor_pointer()
                    .px(px(4.0))
                    .py(px(2.0))
                    .rounded(px(3.0))
                    .text_size(px(11.0))
                    .text_color(t.text_muted)
                    .hover(|h| h.text_color(t.text_primary).bg(t.row_hover))
                    .child("✕")
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |view, _, _window, cx| {
                            cx.stop_propagation();
                            on_clear_click(view, cx);
                        }),
                    ),
            );
        }
    }

    right_icons = right_icons.child(
        div()
            .flex()
            .items_center()
            .justify_center()
            .child(
                gpui::svg()
                    .data(if is_open {
                        crate::icons::CHEVRON_UP_SVG
                    } else {
                        crate::icons::CHEVRON_DOWN_SVG
                    })
                    .size(px(13.0))
                    .text_color(if is_open { t.accent } else { t.text_muted }),
            ),
    );

    trigger_box = trigger_box.child(label_view).child(right_icons);
    col = col.child(trigger_box);

    // Floating popover menu when open
    if is_open {
        let top_offset = if has_label { px(60.0) } else { px(38.0) };
        let on_toggle_close = on_toggle.clone();

        let mut dropdown_menu = div()
            .id(SharedString::from(format!("{id_str}-menu")))
            .occlude()
            .absolute()
            .top(top_offset)
            .left_0()
            .w_full()
            .min_w(px(220.0))
            .rounded(px(8.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.accent)
            .shadow_xl()
            .p(px(6.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .on_mouse_down_out({
                let entity = cx.entity().clone();
                move |_ev, window, cx| {
                    let entity = entity.clone();
                    let on_toggle_close = on_toggle_close.clone();
                    window.defer(cx, move |window, cx| {
                        let _ = entity.update(cx, |view, cx| {
                            on_toggle_close(view, false, window, cx);
                        });
                        window.refresh();
                    });
                }
            });

        // Search row (search_line): magnifying glass icon + unframed search text input
        let search_line = div()
            .id(SharedString::from(format!("{id_str}-search-line")))
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.input_border)
            .child(
                gpui::svg()
                    .data(crate::icons::SEARCH_SVG)
                    .size(px(13.0))
                    .text_color(t.text_muted)
                    .flex_none(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(search_input.clone()),
            );

        dropdown_menu = dropdown_menu.child(search_line);

        // Filter and rank options based on search query
        let query_text = search_input.read(cx).text().trim().to_string();
        let ranked_indices = filter_indices(&query_text, options);

        let mut list_container = div()
            .id(SharedString::from(format!("{id_str}-list")))
            .max_h(px(220.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.0));

        if ranked_indices.is_empty() {
            list_container = list_container.child(
                div()
                    .py(px(14.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .text_color(t.text_muted)
                    .child(empty_hint_str),
            );
        } else {
            for (filtered_pos, &opt_idx) in ranked_indices.iter().enumerate() {
                let opt = &options[opt_idx];
                let is_selected = current_value == Some(opt.as_str());
                let is_active = active_index == Some(filtered_pos);

                let opt_for_click = opt.clone();
                let on_select_click = on_select.clone();

                let mut opt_row = div()
                    .id(SharedString::from(format!("{id_str}-opt-{opt_idx}")))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(if is_selected {
                        t.accent
                    } else {
                        t.text_primary
                    })
                    .bg(if is_active {
                        t.row_hover
                    } else if is_selected {
                        t.accent_subtle
                    } else {
                        t.card_bg
                    })
                    .hover(|h| h.bg(t.row_hover))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .font_weight(if is_selected {
                                gpui::FontWeight::SEMIBOLD
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                            .child(opt.clone()),
                    );

                if let Some(on_set_active_fn) = &on_set_active {
                    let on_hover = on_set_active_fn.clone();
                    opt_row = opt_row.on_mouse_move(cx.listener(move |view, _, _, cx| {
                        on_hover(view, filtered_pos, cx);
                    }));
                }

                if is_selected {
                    opt_row = opt_row.child(
                        gpui::svg()
                            .data(crate::icons::CHECK_SVG)
                            .size(px(12.0))
                            .text_color(t.accent)
                            .flex_none(),
                    );
                }

                opt_row = opt_row.on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |view, _, _window, cx| {
                        cx.stop_propagation();
                        on_select_click(view, opt_for_click.clone(), cx);
                    }),
                );

                list_container = list_container.child(opt_row);
            }
        }

        dropdown_menu = dropdown_menu.child(list_container);
        col = col.child(deferred(dropdown_menu));
    }

    col.into_any_element()
}

// ---------------------------------------------------------------------------
// Standalone Entity: SearchableCombobox
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComboboxEvent {
    Selected(String),
    Cleared,
}

pub struct SearchableCombobox {
    filter: Filter,
    pub state: ComboboxState,
    pub search_input: gpui::Entity<TextInput>,
    pub selected: Option<String>,
    pub placeholder: SharedString,
    pub focus_handle: FocusHandle,
    pub theme: Theme,
}

impl EventEmitter<ComboboxEvent> for SearchableCombobox {}

impl SearchableCombobox {
    pub fn new(
        items: Vec<String>,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        let shared_items: Vec<SharedString> = items.into_iter().map(SharedString::from).collect();
        let search_input = cx.new(|cx| {
            let mut inp = TextInput::new("输入搜索…", cx);
            inp.set_borderless(true);
            inp
        });

        cx.subscribe(
            &search_input,
            |this, _emitter, event: &TextInputEvent, cx| match event {
                TextInputEvent::Change(text) => {
                    this.filter.refilter(text);
                    this.state.active_index = this.filter.active();
                    cx.notify();
                }
                TextInputEvent::Up => {
                    this.filter.step(-1);
                    this.state.active_index = this.filter.active();
                    cx.notify();
                }
                TextInputEvent::Down => {
                    this.filter.step(1);
                    this.state.active_index = this.filter.active();
                    cx.notify();
                }
                TextInputEvent::Enter => {
                    if let Some(val) = this.filter.active_value() {
                        let val_str = val.to_string();
                        this.selected = Some(val_str.clone());
                        this.state.close();
                        cx.emit(ComboboxEvent::Selected(val_str));
                        cx.notify();
                    }
                }
                TextInputEvent::Escape => {
                    this.state.close();
                    cx.notify();
                }
            },
        )
        .detach();

        Self {
            filter: Filter::new(shared_items),
            state: ComboboxState::new(),
            search_input,
            selected: None,
            placeholder: placeholder.into(),
            focus_handle: cx.focus_handle(),
            theme: Theme::dark(),
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn set_items(&mut self, items: Vec<String>, cx: &mut Context<Self>) {
        let shared: Vec<SharedString> = items.into_iter().map(SharedString::from).collect();
        let current_query = self.search_input.read(cx).text().to_string();
        self.filter = Filter::new(shared);
        self.filter.refilter(&current_query);
        self.state.active_index = self.filter.active();
        cx.notify();
    }

    pub fn set_selected(&mut self, selected: Option<String>, cx: &mut Context<Self>) {
        self.selected = selected;
        cx.notify();
    }

    pub fn selected_value(&self) -> Option<&str> {
        self.selected.as_deref()
    }
}

impl Focusable for SearchableCombobox {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SearchableCombobox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme.clone();
        let options: Vec<String> = self.filter.items().iter().map(|s| s.to_string()).collect();
        let is_open = self.state.is_open;
        let active_index = self.state.active_index;
        let selected = self.selected.clone();

        searchable_combobox(
            "combobox",
            None::<&str>,
            selected.as_deref(),
            self.placeholder.clone(),
            &self.search_input,
            is_open,
            &options,
            active_index,
            Some("无匹配项"),
            &t,
            cx,
            |this, open, window, cx| {
                if open {
                    this.state.open();
                    let input = this.search_input.clone();
                    input.update(cx, |inp, cx| {
                        inp.clear(cx);
                        inp.focus_handle.focus(window, cx);
                        inp.start_blink(cx);
                    });
                } else {
                    this.state.close();
                }
                cx.notify();
            },
            |this, _cx| {
                this.state.note_press();
            },
            |this, val, cx| {
                this.selected = Some(val.clone());
                this.state.close();
                cx.emit(ComboboxEvent::Selected(val));
                cx.notify();
            },
            Some(|this: &mut Self, cx: &mut Context<Self>| {
                this.selected = None;
                this.state.close();
                cx.emit(ComboboxEvent::Cleared);
                cx.notify();
            }),
            Some(|this: &mut Self, active_pos: usize, cx: &mut Context<Self>| {
                this.state.active_index = Some(active_pos);
                this.filter.set_active(active_pos);
                cx.notify();
            }),
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_rank_prefix_substring_mismatch() {
        assert_eq!(match_rank("claude", "claude-3-5-sonnet"), Some(0));
        assert_eq!(match_rank("sonnet", "claude-3-5-sonnet"), Some(1));
        assert_eq!(match_rank("gpt", "claude-3-5-sonnet"), None);
        assert_eq!(match_rank("", "claude-3-5-sonnet"), Some(1));
    }

    #[test]
    fn test_filter_indices_ranking() {
        let labels = vec![
            "my-custom-model",
            "openai-gpt-4o",
            "anthropic-claude",
            "claude-3-5-sonnet",
        ];
        // Searching for "claude": "claude-3-5-sonnet" (prefix) should rank before "anthropic-claude" (substring)
        let filtered = filter_indices("claude", &labels);
        assert_eq!(filtered, vec![3, 2]);
    }

    #[test]
    fn test_filter_navigation_and_active_item() {
        let items = vec![
            SharedString::from("alpha"),
            SharedString::from("beta"),
            SharedString::from("gamma"),
        ];
        let mut filter = Filter::new(items);
        assert_eq!(filter.active(), Some(0));
        assert_eq!(filter.active_item(), Some(0));

        filter.step(1);
        assert_eq!(filter.active(), Some(1));
        assert_eq!(filter.active_item(), Some(1));

        filter.step(1);
        assert_eq!(filter.active(), Some(2));
        assert_eq!(filter.active_item(), Some(2));

        // Wraps around
        filter.step(1);
        assert_eq!(filter.active(), Some(0));
        assert_eq!(filter.active_item(), Some(0));

        // Step backwards wraps to end
        filter.step(-1);
        assert_eq!(filter.active(), Some(2));
        assert_eq!(filter.active_item(), Some(2));
    }

    #[test]
    fn test_combobox_state_toggle_safety() {
        let mut state = ComboboxState::new();
        assert!(!state.is_open);

        // Press while closed:
        state.note_press();
        assert!(!state.pressed_while_open);
        // Toggle opens:
        assert!(state.toggle());
        assert!(state.is_open);

        // Press while open:
        state.note_press();
        assert!(state.pressed_while_open);
        // An outside dismiss occurs:
        state.close();
        assert!(!state.is_open);
        // But pressed_while_open was recorded as true, so release does NOT reopen:
        assert!(!state.toggle());
        assert!(!state.is_open);
    }

    #[test]
    fn test_match_model_rank_and_filter() {
        use aitoolplus_core::api_hub::FetchedModel;

        let models = vec![
            FetchedModel {
                id: "gpt-4o".to_string(),
                display_name: Some("GPT-4o".to_string()),
                owned_by: Some("openai".to_string()),
                context_length: Some(128000),
                ..Default::default()
            },
            FetchedModel {
                id: "claude-3-5-sonnet-20241022".to_string(),
                display_name: Some("Claude 3.5 Sonnet".to_string()),
                owned_by: Some("anthropic".to_string()),
                context_length: Some(200000),
                ..Default::default()
            },
            FetchedModel {
                id: "anthropic-claude-instant".to_string(),
                display_name: Some("Claude Instant".to_string()),
                owned_by: Some("anthropic".to_string()),
                context_length: Some(100000),
                ..Default::default()
            },
        ];

        // "claude": "claude-3-5-sonnet" (id prefix match, rank 0) should rank before
        // "anthropic-claude-instant" (id substring match, rank 1)
        let filtered = filter_models("claude", &models);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].id, "claude-3-5-sonnet-20241022");
        assert_eq!(filtered[1].id, "anthropic-claude-instant");

        // Vendor match "openai"
        let filtered_openai = filter_models("openai", &models);
        assert_eq!(filtered_openai.len(), 1);
        assert_eq!(filtered_openai[0].id, "gpt-4o");
    }
}
