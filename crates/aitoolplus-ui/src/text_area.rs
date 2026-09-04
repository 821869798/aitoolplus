//! Multi-line plain-text editor for JSON configs and prompts.
//!
//! GPUI has no built-in multiline input, so this is a compact element that
//! renders lines, supports caret movement, IME via `EntityInputHandler`, and
//! change events. Enough for editing config blobs; not a code editor.

use std::ops::Range;
use std::time::Duration;

use gpui::{
    App, Bounds, ClipboardItem, Context, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, IntoElement, LayoutId, PaintQuad,
    Pixels, Point, SharedString, Style, TextRun, UTF16Selection, Window, fill, hsla, point, px,
    relative, rgba, size,
};
use unicode_segmentation::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextAreaEvent {
    Change(String),
    Escape,
}

/// Newline as a char const, to keep quoting sane in this file.
pub const NL_CH: char = '\n';

/// Byte offset of the start of the line containing `offset`.
fn line_start_of(content: &str, offset: usize) -> usize {
    let o = offset.min(content.len());
    content[..o].rfind(NL_CH).map(|i| i + 1).unwrap_or(0)
}

/// Byte offset just past the newline ending the line containing `offset`
/// (or the end of content).
fn line_end_of(content: &str, offset: usize) -> usize {
    let o = offset.min(content.len());
    content[o..]
        .find(NL_CH)
        .map(|i| o + i)
        .unwrap_or(content.len())
}

pub struct TextArea {
    pub focus_handle: FocusHandle,
    pub content: String,
    pub placeholder: String,
    pub selected_range: Range<usize>,
    pub marked_range: Option<Range<usize>>,
    pub cursor_visible: bool,
    pub max_lines: usize,
    _blink_task: Option<gpui::Task<()>>,
}

impl gpui::EventEmitter<TextAreaEvent> for TextArea {}

impl TextArea {
    pub fn new(placeholder: impl Into<String>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: String::new(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            marked_range: None,
            cursor_visible: false,
            max_lines: 14,
            _blink_task: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
        cx.emit(TextAreaEvent::Change(self.content.clone()));
        cx.notify();
    }

    pub fn set_text_silent(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
        cx.notify();
    }

    pub fn set_max_lines(&mut self, lines: usize, cx: &mut Context<Self>) {
        self.max_lines = lines;
        cx.notify();
    }

    fn start_blink(&mut self, cx: &mut Context<Self>) {
        if self._blink_task.is_some() {
            return;
        }
        self.cursor_visible = true;
        self._blink_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let res = this.update(cx, |ta, cx| {
                    ta.cursor_visible = !ta.cursor_visible;
                    cx.notify();
                });
                if res.is_err() {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn stop_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = false;
        self._blink_task = None;
        cx.notify();
    }

    fn reset_blink(&mut self, cx: &mut Context<Self>) {
        self.stop_blink(cx);
        self.start_blink(cx);
    }

    pub fn handle_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();

        if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
            match key {
                "a" | "A" => {
                    self.selected_range = 0..self.content.len();
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
                        self.replace_text_in_range(None, &text, window, cx);
                    }
                    return;
                }
                _ => {}
            }
        }

        match key {
            "left" => self.move_cursor(self.prev_grapheme(self.cursor()), cx),
            "right" => self.move_cursor(self.next_grapheme(self.cursor()), cx),
            "up" => self.move_cursor(self.prev_line_start(self.cursor()), cx),
            "down" => self.move_cursor(self.next_line_start(self.cursor()), cx),
            "home" => self.move_cursor(self.line_start(self.cursor()), cx),
            "end" => self.move_cursor(self.line_end(self.cursor()), cx),
            "backspace" => {
                if self.selected_range.is_empty() && self.cursor() > 0 {
                    let prev = self.prev_grapheme(self.cursor());
                    self.selected_range = prev..self.cursor();
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "delete" => {
                if self.selected_range.is_empty() && self.cursor() < self.content.len() {
                    let next = self.next_grapheme(self.cursor());
                    self.selected_range = self.cursor()..next;
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "escape" => cx.emit(TextAreaEvent::Escape),
            _ => {}
        }
    }

    fn cursor(&self) -> usize {
        self.selected_range.end
    }

    fn move_cursor(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selected_range = offset..offset;
        self.reset_blink(cx);
        cx.notify();
    }

    fn prev_grapheme(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_grapheme(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    fn line_start(&self, offset: usize) -> usize {
        line_start_of(&self.content, offset)
    }

    fn line_end(&self, offset: usize) -> usize {
        line_end_of(&self.content, offset)
    }

    fn prev_line_start(&self, offset: usize) -> usize {
        let cur = self.line_start(offset);
        if cur == 0 {
            0
        } else {
            self.line_start(cur - 1)
        }
    }

    fn next_line_start(&self, offset: usize) -> usize {
        let end = self.line_end(offset);
        if end >= self.content.len() {
            self.content.len()
        } else {
            end + 1
        }
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

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

impl Focusable for TextArea {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for TextArea {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        let s = range.start.min(self.content.len());
        let e = range.end.min(self.content.len());
        Some(self.content[s..e].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| self.range_to_utf16(r))
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
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
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
        let cursor = start + new_text.len();
        self.selected_range = cursor..cursor;
        self.marked_range = None;
        self.reset_blink(cx);
        cx.emit(TextAreaEvent::Change(self.content.clone()));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range_utf16, new_text, _window, cx);
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }
}

impl gpui::Render for TextArea {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        TextAreaElement { input: entity }
    }
}

pub struct TextAreaElement {
    pub input: Entity<TextArea>,
}

impl IntoElement for TextAreaElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

pub struct TextAreaPrepaint {
    lines: Vec<gpui::ShapedLine>,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
}

impl Element for TextAreaElement {
    type RequestLayoutState = ();
    type PrepaintState = TextAreaPrepaint;

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
        let ta = self.input.read(cx);
        let line_count = ta.content.lines().count().max(1).min(ta.max_lines.max(1));
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let line_height = window.line_height();
        let total_h = line_height * line_count as f32 + px(8.0);
        style.size.height = total_h.into();
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
        let ta = self.input.read(cx);
        let is_focused = ta.focus_handle.is_focused(window);
        if !is_focused && ta._blink_task.is_some() {
            self.input.update(cx, |ta, cx| ta.stop_blink(cx));
        }
        let ta = self.input.read(cx);
        let content = ta.content.clone();
        let placeholder = ta.placeholder.clone();
        let selected = ta.selected_range.clone();
        let cursor_visible = ta.cursor_visible && is_focused;
        let cursor = ta.selected_range.end;

        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();

        let mut lines = vec![];
        let mut selections = vec![];
        let mut cursor_quad = None;

        let all_lines: Vec<String> = if content.is_empty() {
            vec![placeholder.clone()]
        } else {
            content.split(NL_CH).map(String::from).collect()
        };

        let mut byte_offset = 0usize;
        for (line_idx, line_text) in all_lines.iter().enumerate() {
            let line_start = byte_offset;
            let line_end = line_start + line_text.len();
            byte_offset = line_end + 1;

            let is_placeholder = content.is_empty();
            let color = if is_placeholder {
                hsla(0., 0., 0.45, 0.7)
            } else {
                style.color
            };
            let runs: Vec<TextRun> = if line_text.is_empty() {
                vec![]
            } else {
                vec![TextRun {
                    len: line_text.len(),
                    font: style.font(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                    letter_spacing: None,
                }]
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(line_text.clone()),
                font_size,
                if runs.is_empty() {
                    &[] as &[TextRun]
                } else {
                    &runs
                },
                None,
            );

            let sel_start = selected.start.max(line_start).min(line_end);
            let sel_end = selected.end.max(line_start).min(line_end);
            if !content.is_empty() && sel_end > sel_start {
                let x0 = shaped.x_for_index(sel_start - line_start);
                let x1 = shaped.x_for_index(sel_end - line_start);
                let top = bounds.top() + line_height * line_idx as f32;
                selections.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + x0, top),
                        point(bounds.left() + x1, top + line_height),
                    ),
                    rgba(0x3b82f64d),
                ));
            }

            if cursor_visible && cursor >= line_start && cursor <= line_end {
                let cx_pos = shaped.x_for_index((cursor - line_start).min(line_text.len()));
                let top = bounds.top() + line_height * line_idx as f32;
                cursor_quad = Some(fill(
                    Bounds::new(
                        point(bounds.left() + cx_pos, top + px(1.0)),
                        size(px(1.5), line_height - px(2.0)),
                    ),
                    style.color,
                ));
            }

            lines.push(shaped);
        }

        TextAreaPrepaint {
            lines,
            cursor: cursor_quad,
            selections,
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

        for sel in prepaint.selections.drain(..) {
            window.paint_quad(sel);
        }

        let line_height = window.line_height();
        for (idx, line) in prepaint.lines.drain(..).enumerate() {
            let top = bounds.top() + line_height * idx as f32;
            let _ = line.paint(
                point(bounds.left(), top),
                line_height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
        }

        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_navigation_helpers() {
        let content = format!("line one{}line two{}line three", NL_CH, NL_CH);
        assert_eq!(line_start_of(&content, 0), 0);
        assert_eq!(line_end_of(&content, 0), 8);
        assert_eq!(line_start_of(&content, 9), 9);
        assert_eq!(line_end_of(&content, 0), 8);
        // next line start from 0 -> after first newline
        assert_eq!(line_end_of(&content, 0) + 1, 9);
        assert_eq!(line_start_of(&content, 18), 18);
        assert_eq!(line_start_of(&content, 9), 9);
        // previous line start from the third line
        assert_eq!(line_start_of(&content, 17), 9);
        assert_eq!(line_start_of(&content, 8), 0);
        // end of buffer
        assert_eq!(line_end_of(&content, 18), content.len());
    }
}
