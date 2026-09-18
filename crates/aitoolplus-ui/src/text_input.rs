use std::ops::Range;
use std::time::Duration;

use gpui::{
    App, Bounds, ClipboardItem, Context, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyDownEvent, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style,
    TextRun, UTF16Selection, UnderlineStyle, Window, div, fill, hsla, point, prelude::*, px,
    relative, rgba, size,
};
use unicode_segmentation::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
    Change(String),
    Escape,
    Enter,
}

pub struct TextInput {
    pub focus_handle: FocusHandle,
    pub content: String,
    pub placeholder: String,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub last_layout: Option<ShapedLine>,
    pub last_bounds: Option<Bounds<Pixels>>,
    pub is_secret: bool,
    pub read_only: bool,
    pub cursor_visible: bool,
    pub drag_anchor: Option<usize>,
    _blink_task: Option<gpui::Task<()>>,
}

impl gpui::EventEmitter<TextInputEvent> for TextInput {}

impl TextInput {
    pub fn new(placeholder: impl Into<String>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        Self {
            focus_handle,
            content: String::new(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_secret: false,
            read_only: false,
            cursor_visible: false,
            drag_anchor: None,
            _blink_task: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn is_blinking(&self) -> bool {
        self._blink_task.is_some()
    }

    pub fn set_secret(&mut self, is_secret: bool, cx: &mut Context<Self>) {
        self.is_secret = is_secret;
        cx.notify();
    }

    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.read_only = read_only;
        cx.notify();
    }

    /// Sets the text and emits a [`TextInputEvent::Change`] event.
    ///
    /// Use this for programmatic user-like inputs. To synchronize UI with
    /// external settings without triggering save/recompile cycles, use [`set_text_silent`].
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
        cx.emit(TextInputEvent::Change(self.content.clone()));
        cx.notify();
    }

    /// Sets the text silently without emitting a change event.
    pub fn set_text_silent(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
        cx.notify();
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>, cx: &mut Context<Self>) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content.clear();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        cx.emit(TextInputEvent::Change(self.content.clone()));
        cx.notify();
    }

    pub fn start_blink(&mut self, cx: &mut Context<Self>) {
        if self._blink_task.is_some() {
            return;
        }
        self.cursor_visible = true;
        self._blink_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let res = this.update(cx, |input, cx| {
                    input.cursor_visible = !input.cursor_visible;
                    cx.notify();
                });
                if res.is_err() {
                    break;
                }
            }
        }));
        cx.notify();
    }

    pub fn stop_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = false;
        self._blink_task = None;
        cx.notify();
    }

    pub fn reset_blink(&mut self, cx: &mut Context<Self>) {
        self.stop_blink(cx);
        self.start_blink(cx);
    }

    pub fn content_offset_to_shaped_offset(&self, content_offset: usize) -> usize {
        content_offset_to_shaped(&self.content, content_offset, self.is_secret)
    }

    pub fn shaped_offset_to_content_offset(&self, shaped_offset: usize) -> usize {
        shaped_offset_to_content(&self.content, shaped_offset, self.is_secret)
    }

    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selected_range = offset..offset;
        self.reset_blink(cx);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.reset_blink(cx);
        cx.notify();
    }

    pub fn handle_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();

        if self.read_only {
            if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
                match key {
                    "a" | "A" => {
                        self.selected_range = 0..self.content.len();
                        self.selection_reversed = false;
                        cx.notify();
                        return;
                    }
                    "c" | "C" => {
                        if !self.selected_range.is_empty() {
                            let text = self.content[self.selected_range.clone()].to_string();
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                        }
                        return;
                    }
                    _ => {}
                }
            }
            match key {
                "left" => {
                    if event.keystroke.modifiers.shift {
                        let prev = self.previous_boundary(self.cursor_offset());
                        self.select_to(prev, cx);
                    } else if self.selected_range.is_empty() {
                        let prev = self.previous_boundary(self.cursor_offset());
                        self.move_to(prev, cx);
                    } else {
                        self.move_to(self.selected_range.start, cx);
                    }
                }
                "right" => {
                    if event.keystroke.modifiers.shift {
                        let next = self.next_boundary(self.cursor_offset());
                        self.select_to(next, cx);
                    } else if self.selected_range.is_empty() {
                        let next = self.next_boundary(self.cursor_offset());
                        self.move_to(next, cx);
                    } else {
                        self.move_to(self.selected_range.end, cx);
                    }
                }
                "home" => {
                    if event.keystroke.modifiers.shift {
                        self.select_to(0, cx);
                    } else {
                        self.move_to(0, cx);
                    }
                }
                "end" => {
                    let len = self.content.len();
                    if event.keystroke.modifiers.shift {
                        self.select_to(len, cx);
                    } else {
                        self.move_to(len, cx);
                    }
                }
                "escape" => {
                    cx.emit(TextInputEvent::Escape);
                }
                _ => {}
            }
            return;
        }

        if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
            match key {
                "a" | "A" => {
                    self.selected_range = 0..self.content.len();
                    self.selection_reversed = false;
                    cx.notify();
                    return;
                }
                "c" | "C" => {
                    if !self.selected_range.is_empty() {
                        let text = self.content[self.selected_range.clone()].to_string();
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    return;
                }
                "x" | "X" => {
                    if !self.selected_range.is_empty() {
                        let text = self.content[self.selected_range.clone()].to_string();
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        self.replace_text_in_range(None, "", window, cx);
                    }
                    return;
                }
                "v" | "V" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        let clean = text.replace(['\r', '\n'], " ");
                        self.replace_text_in_range(None, &clean, window, cx);
                    }
                    return;
                }
                _ => {}
            }
        }

        match key {
            "left" => {
                if event.keystroke.modifiers.shift {
                    let prev = self.previous_boundary(self.cursor_offset());
                    self.select_to(prev, cx);
                } else if self.selected_range.is_empty() {
                    let prev = self.previous_boundary(self.cursor_offset());
                    self.move_to(prev, cx);
                } else {
                    self.move_to(self.selected_range.start, cx);
                }
            }
            "right" => {
                if event.keystroke.modifiers.shift {
                    let next = self.next_boundary(self.cursor_offset());
                    self.select_to(next, cx);
                } else if self.selected_range.is_empty() {
                    let next = self.next_boundary(self.cursor_offset());
                    self.move_to(next, cx);
                } else {
                    self.move_to(self.selected_range.end, cx);
                }
            }
            "home" => {
                if event.keystroke.modifiers.shift {
                    self.select_to(0, cx);
                } else {
                    self.move_to(0, cx);
                }
            }
            "end" => {
                let len = self.content.len();
                if event.keystroke.modifiers.shift {
                    self.select_to(len, cx);
                } else {
                    self.move_to(len, cx);
                }
            }
            "backspace" => {
                if self.selected_range.is_empty() {
                    let prev = self.previous_boundary(self.cursor_offset());
                    if self.cursor_offset() > 0 {
                        self.selected_range = prev..self.cursor_offset();
                    }
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "delete" => {
                if self.selected_range.is_empty() {
                    let next = self.next_boundary(self.cursor_offset());
                    if self.cursor_offset() < self.content.len() {
                        self.selected_range = self.cursor_offset()..next;
                    }
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "escape" => {
                cx.emit(TextInputEvent::Escape);
            }
            "enter" => {
                cx.emit(TextInputEvent::Enter);
            }
            _ => {}
        }
    }

    pub fn on_mouse_down(&mut self, position: Point<Pixels>, click_count: usize, cx: &mut Context<Self>) {
        if self.content.is_empty() {
            self.selected_range = 0..0;
            self.selection_reversed = false;
            self.drag_anchor = Some(0);
            self.reset_blink(cx);
            return;
        }

        let offset = self.index_for_mouse_position(position);

        match click_count {
            1 => {
                self.selected_range = offset..offset;
                self.selection_reversed = false;
                self.drag_anchor = Some(offset);
            }
            2 => {
                let range = word_bounds_at(&self.content, offset);
                self.selected_range = range.clone();
                self.selection_reversed = false;
                self.drag_anchor = Some(range.start);
            }
            _ => {
                self.selected_range = 0..self.content.len();
                self.selection_reversed = false;
                self.drag_anchor = Some(0);
            }
        }
        self.reset_blink(cx);
    }

    pub fn on_mouse_move(&mut self, position: Point<Pixels>, is_left_down: bool, cx: &mut Context<Self>) {
        if !is_left_down {
            self.drag_anchor = None;
            return;
        }
        let Some(anchor) = self.drag_anchor else {
            return;
        };
        let curr = self.index_for_mouse_position(position);
        if curr >= anchor {
            self.selected_range = anchor..curr;
            self.selection_reversed = false;
        } else {
            self.selected_range = curr..anchor;
            self.selection_reversed = true;
        }
        self.reset_blink(cx);
        cx.notify();
    }

    pub fn on_mouse_up(&mut self, _position: Point<Pixels>, cx: &mut Context<Self>) {
        self.drag_anchor = None;
        cx.notify();
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.x <= bounds.left() {
            return 0;
        }
        if position.x >= bounds.right() {
            return self.content.len();
        }
        let shaped_idx = line.closest_index_for_x(position.x - bounds.left());
        self.shaped_offset_to_content_offset(shaped_idx)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        previous_boundary_of(&self.content, offset)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        next_boundary_of(&self.content, offset)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

pub fn word_bounds_at(content: &str, offset: usize) -> Range<usize> {
    if content.is_empty() {
        return 0..0;
    }
    let o = offset.min(content.len());
    let mut best_range = 0..content.len();
    for (idx, word) in content.split_word_bound_indices() {
        let word_end = idx + word.len();
        if idx <= o && o <= word_end {
            best_range = idx..word_end;
            break;
        }
    }
    best_range
}

impl Focusable for TextInput {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len());
        self.content = format!(
            "{}{}{}",
            &self.content[..start],
            new_text,
            &self.content[end..]
        );
        let new_cursor = start + new_text.len();
        self.selected_range = new_cursor..new_cursor;
        self.marked_range.take();
        self.reset_blink(cx);
        cx.emit(TextInputEvent::Change(self.content.clone()));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len());
        self.content = format!(
            "{}{}{}",
            &self.content[..start],
            new_text,
            &self.content[end..]
        );
        self.marked_range = Some(start..start + new_text.len());
        self.selected_range = if let Some(r) = new_selected_range_utf16 {
            let mut utf16_count = 0;
            let mut rel_start = new_text.len();
            let mut rel_end = new_text.len();
            for (idx, ch) in new_text.char_indices() {
                if utf16_count == r.start {
                    rel_start = idx;
                }
                if utf16_count == r.end {
                    rel_end = idx;
                }
                utf16_count += ch.len_utf16();
            }
            if utf16_count == r.start {
                rel_start = new_text.len();
            }
            if utf16_count == r.end {
                rel_end = new_text.len();
            }
            (start + rel_start)..(start + rel_end)
        } else {
            (start + new_text.len())..(start + new_text.len())
        };
        self.reset_blink(cx);
        cx.emit(TextInputEvent::Change(self.content.clone()));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let shaped_start = self.content_offset_to_shaped_offset(range.start);
        let shaped_end = self.content_offset_to_shaped_offset(range.end);
        let start_x = last_layout.x_for_index(shaped_start);
        let end_x = last_layout.x_for_index(shaped_end);
        Some(Bounds::from_corners(
            point(bounds.left() + start_x, bounds.top()),
            point(bounds.left() + end_x, bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let last_bounds = self.last_bounds.as_ref()?;
        let last_layout = self.last_layout.as_ref()?;
        if point.x <= last_bounds.left() {
            return Some(0);
        }
        if point.x >= last_bounds.right() {
            return Some(self.offset_to_utf16(self.content.len()));
        }
        let shaped_idx = last_layout.closest_index_for_x(point.x - last_bounds.left());
        let content_idx = self.shaped_offset_to_content_offset(shaped_idx);
        Some(self.offset_to_utf16(content_idx))
    }
}

impl gpui::Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let is_focused = self.focus_handle.is_focused(window);
        div()
            .id(("text_input_field", entity.entity_id()))
            .key_context("TextInput")
            .track_focus(&self.focus_handle)
            .cursor_text()
            .size_full()
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .when(is_focused, |d| {
                d.border_1().border_color(rgba(0x3b82f6cc))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.start_blink(cx);
                    this.on_mouse_down(event.position, event.click_count, cx);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                let is_left = event.pressed_button == Some(MouseButton::Left);
                this.on_mouse_move(event.position, is_left, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                    this.on_mouse_up(event.position, cx);
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key_down(event, window, cx);
            }))
            .child(TextInputElement { input: entity })
    }
}

pub struct TextInputElement {
    pub input: Entity<TextInput>,
}

pub struct TextInputPrepaint {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextInputElement {
    type RequestLayoutState = ();
    type PrepaintState = TextInputPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let is_focused = input.focus_handle.is_focused(window);
        if !is_focused && input.is_blinking() {
            self.input.update(cx, |input, cx| {
                input.stop_blink(cx);
            });
        }
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let cursor_visible = input.cursor_visible;

        let style = window.text_style();
        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), hsla(0., 0., 0.5, 0.6))
        } else if input.is_secret {
            ("•".repeat(content.chars().count()), style.color)
        } else {
            (content.clone(), style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end.saturating_sub(marked_range.start),
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len().saturating_sub(marked_range.end),
                    ..run
                },
            ]
            .into_iter()
            .filter(|r| r.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window.text_system().shape_line(
            SharedString::from(display_text),
            font_size,
            &runs,
            None,
        );

        let shaped_cursor = input.content_offset_to_shaped_offset(cursor);
        let shaped_sel_start = input.content_offset_to_shaped_offset(selected_range.start);
        let shaped_sel_end = input.content_offset_to_shaped_offset(selected_range.end);

        let cursor_pos = line.x_for_index(shaped_cursor);
        let (selection, cursor) = if !content.is_empty() && !selected_range.is_empty() {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(shaped_sel_start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(shaped_sel_end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x3b82f640),
                )),
                None,
            )
        } else if is_focused && cursor_visible && !input.read_only {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top() + px(1.0)),
                        size(px(1.5), bounds.bottom() - bounds.top() - px(2.0)),
                    ),
                    style.color,
                )),
            )
        } else {
            (None, None)
        };

        TextInputPrepaint {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        if let Some(line) = prepaint.line.take() {
            let line_height = window.line_height();
            let _ = line.paint(
                point(bounds.left(), bounds.top()),
                line_height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
            self.input.update(cx, |input, _| {
                input.last_layout = Some(line);
                input.last_bounds = Some(bounds);
            });
        }

        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}

pub fn content_offset_to_shaped(content: &str, content_offset: usize, is_secret: bool) -> usize {
    if !is_secret || content.is_empty() {
        return content_offset.min(content.len());
    }
    let safe_offset = content.floor_char_boundary(content_offset.min(content.len()));
    let char_idx = content[..safe_offset].chars().count();
    char_idx * "•".len()
}

pub fn shaped_offset_to_content(content: &str, shaped_offset: usize, is_secret: bool) -> usize {
    if !is_secret || content.is_empty() {
        return shaped_offset.min(content.len());
    }
    let char_idx = shaped_offset / "•".len();
    content
        .char_indices()
        .nth(char_idx)
        .map_or(content.len(), |(i, _)| i)
}

pub fn previous_boundary_of(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

pub fn next_boundary_of(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(content.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_offset_bidirectional_mapping() {
        let content = "ab中cd";
        // "ab中cd": 'a' (0..1), 'b' (1..2), '中' (2..5), 'c' (5..6), 'd' (6..7)
        // Masked text is "•••••", each • is 3 bytes (0, 3, 6, 9, 12, 15)
        assert_eq!(content_offset_to_shaped(content, 0, true), 0);
        assert_eq!(content_offset_to_shaped(content, 1, true), 3);
        assert_eq!(content_offset_to_shaped(content, 2, true), 6);
        assert_eq!(content_offset_to_shaped(content, 3, true), 6);
        assert_eq!(content_offset_to_shaped(content, 5, true), 9);
        assert_eq!(content_offset_to_shaped(content, 6, true), 12);
        assert_eq!(content_offset_to_shaped(content, 7, true), 15);

        assert_eq!(shaped_offset_to_content(content, 0, true), 0);
        assert_eq!(shaped_offset_to_content(content, 3, true), 1);
        assert_eq!(shaped_offset_to_content(content, 6, true), 2);
        assert_eq!(shaped_offset_to_content(content, 9, true), 5);
        assert_eq!(shaped_offset_to_content(content, 12, true), 6);
        assert_eq!(shaped_offset_to_content(content, 15, true), 7);
    }

    #[test]
    fn test_boundary_navigation() {
        let text = "hello 世界 test";
        // 'hello ' (0..6), '世界 ' (6..13), 'test' (13..17)
        assert_eq!(previous_boundary_of(text, 6), 5);
        assert_eq!(next_boundary_of(text, 0), 1);
        assert_eq!(next_boundary_of(text, 6), 9); // '世' is 3 bytes, starts at 6, next is 9
        assert_eq!(next_boundary_of(text, 9), 12); // '界' is 3 bytes, starts at 9, next is 12
    }

    #[test]
    fn test_word_bounds_selection() {
        let text = "hello world rust";
        assert_eq!(word_bounds_at(text, 2), 0..5);
        assert_eq!(word_bounds_at(text, 7), 6..11);
        assert_eq!(word_bounds_at(text, 14), 12..16);
    }
}
