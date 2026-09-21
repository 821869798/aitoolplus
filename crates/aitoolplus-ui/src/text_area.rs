//! Multi-line plain-text editor for JSON configs and prompts.
//!
//! GPUI has no built-in multiline input, so this is a compact element that
//! renders lines, supports caret movement, IME via `EntityInputHandler`,
//! soft wrapping, and change events. Enough for editing config blobs; not a code editor.

use std::ops::Range;
use std::time::Duration;

use gpui::{
    App, Bounds, ClipboardItem, Context, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, IntoElement, KeyDownEvent,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, div, fill, hsla, point,
    prelude::*, px, relative, rgba, size,
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

/// Helper to slice TextRuns matching a given byte range.
fn runs_for_slice(runs: &[TextRun], start: usize, len: usize) -> Vec<TextRun> {
    let mut result = Vec::new();
    let mut run_start = 0;
    let target_end = start + len;
    for run in runs {
        let run_end = run_start + run.len;
        let overlap_start = run_start.max(start);
        let overlap_end = run_end.min(target_end);
        if overlap_end > overlap_start {
            let mut r = run.clone();
            r.len = overlap_end - overlap_start;
            result.push(r);
        }
        run_start = run_end;
    }
    result
}

/// Wrap a single logical line into multiple visual lines when it exceeds `max_width`.
fn wrap_logical_line(
    logical_text: &str,
    logical_start: usize,
    max_width: Pixels,
    font_size: Pixels,
    runs: &[TextRun],
    window: &mut Window,
) -> Vec<(Range<usize>, ShapedLine)> {
    if logical_text.is_empty() {
        let shaped = window.text_system().shape_line(
            SharedString::from(""),
            font_size,
            &[] as &[TextRun],
            None,
        );
        return vec![(logical_start..logical_start, shaped)];
    }

    let full_shaped = window.text_system().shape_line(
        SharedString::from(logical_text.to_string()),
        font_size,
        runs,
        None,
    );

    if max_width <= px(0.0) || full_shaped.width <= max_width {
        return vec![(logical_start..logical_start + logical_text.len(), full_shaped)];
    }

    let mut visual = Vec::new();
    let mut cur_local = 0usize;
    let total_len = logical_text.len();

    while cur_local < total_len {
        let rem_slice = &logical_text[cur_local..];
        let rem_runs = runs_for_slice(runs, cur_local, rem_slice.len());
        let rem_shaped = window.text_system().shape_line(
            SharedString::from(rem_slice.to_string()),
            font_size,
            &rem_runs,
            None,
        );

        if rem_shaped.width <= max_width {
            visual.push((
                (logical_start + cur_local)..(logical_start + total_len),
                rem_shaped,
            ));
            break;
        }

        // Find break point within rem_slice
        let closest_idx = rem_shaped.closest_index_for_x(max_width);
        let mut break_idx = closest_idx.max(1).min(rem_slice.len());
        // Align to valid UTF-8 character boundary
        while break_idx < rem_slice.len() && !rem_slice.is_char_boundary(break_idx) {
            break_idx += 1;
        }
        if break_idx >= rem_slice.len() {
            visual.push((
                (logical_start + cur_local)..(logical_start + total_len),
                rem_shaped,
            ));
            break;
        }

        // For Western text, break at word boundary if available within recent span
        if let Some(space_pos) = rem_slice[..break_idx].rfind(' ') {
            if space_pos > 0 && space_pos >= break_idx.saturating_sub(24) {
                break_idx = space_pos + 1;
            }
        }

        let seg_slice = &rem_slice[..break_idx];
        let seg_runs = runs_for_slice(runs, cur_local, break_idx);
        let seg_shaped = window.text_system().shape_line(
            SharedString::from(seg_slice.to_string()),
            font_size,
            &seg_runs,
            None,
        );

        visual.push((
            (logical_start + cur_local)..(logical_start + cur_local + break_idx),
            seg_shaped,
        ));
        cur_local += break_idx;
    }

    if visual.is_empty() {
        visual.push((logical_start..logical_start + logical_text.len(), full_shaped));
    }

    visual
}

pub struct TextArea {
    pub focus_handle: FocusHandle,
    pub content: String,
    pub placeholder: String,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub cursor_visible: bool,
    pub max_lines: usize,
    pub last_bounds: Option<Bounds<Pixels>>,
    pub line_height: Option<Pixels>,
    pub last_layouts: Vec<ShapedLine>,
    pub visual_lines: Vec<(Range<usize>, ShapedLine)>,
    pub soft_wrap: bool,
    pub drag_anchor: Option<usize>,
    pub read_only: bool,
    pub borderless: bool,
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
            selection_reversed: false,
            marked_range: None,
            cursor_visible: false,
            max_lines: 14,
            last_bounds: None,
            line_height: None,
            last_layouts: Vec::new(),
            visual_lines: Vec::new(),
            soft_wrap: true,
            drag_anchor: None,
            read_only: false,
            borderless: true,
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

    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.read_only = read_only;
        cx.notify();
    }

    pub fn set_borderless(&mut self, borderless: bool, cx: &mut Context<Self>) {
        self.borderless = borderless;
        cx.notify();
    }

    pub fn set_soft_wrap(&mut self, soft_wrap: bool, cx: &mut Context<Self>) {
        self.soft_wrap = soft_wrap;
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

    pub fn stop_blink(&mut self, cx: &mut Context<Self>) {
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
        // When IME composition is active, let IME handle keystrokes directly
        if self.marked_range.is_some() {
            return;
        }

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
                "left" => self.move_cursor(self.prev_grapheme(self.cursor()), cx),
                "right" => self.move_cursor(self.next_grapheme(self.cursor()), cx),
                "up" => self.move_cursor(self.prev_line_start(self.cursor()), cx),
                "down" => self.move_cursor(self.next_line_start(self.cursor()), cx),
                "home" => self.move_cursor(self.line_start(self.cursor()), cx),
                "end" => self.move_cursor(self.line_end(self.cursor()), cx),
                "escape" => cx.emit(TextAreaEvent::Escape),
                _ => {}
            }
            return;
        }

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
            "enter" => {
                self.replace_text_in_range(None, "\n", window, cx);
            }
            "tab" => {
                self.replace_text_in_range(None, "  ", window, cx);
            }
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

    pub fn on_mouse_down(
        &mut self,
        position: Point<Pixels>,
        click_count: usize,
        cx: &mut Context<Self>,
    ) {
        if self.content.is_empty() {
            self.selected_range = 0..0;
            self.selection_reversed = false;
            self.drag_anchor = Some(0);
            self.cursor_visible = true;
            self.reset_blink(cx);
            cx.notify();
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
                let start = self.line_start(offset);
                let end = (self.line_end(offset) + 1).min(self.content.len());
                self.selected_range = start..end;
                self.selection_reversed = false;
                self.drag_anchor = Some(start);
            }
        }
        self.cursor_visible = true;
        self.reset_blink(cx);
        cx.notify();
    }

    pub fn on_mouse_move(
        &mut self,
        position: Point<Pixels>,
        is_left_down: bool,
        cx: &mut Context<Self>,
    ) {
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
        let Some(bounds) = self.last_bounds.as_ref() else {
            return 0;
        };
        // Above the text area bounds
        if position.y <= bounds.top() {
            return 0;
        }

        if !self.visual_lines.is_empty() {
            let total_lines = self.visual_lines.len();
            let line_h = self.line_height.unwrap_or(px(20.0));
            let lines_h = line_h * total_lines as f32;

            // If mouse is below all visual lines of text or below bounds, cursor belongs at the end of the text
            if position.y >= bounds.top() + lines_h || position.y >= bounds.bottom() {
                return self.content.len();
            }

            let raw_idx = if line_h > px(0.0) {
                ((position.y - bounds.top()) / line_h).floor() as isize
            } else {
                0
            };
            if raw_idx < 0 {
                return 0;
            }
            if raw_idx as usize >= total_lines {
                return self.content.len();
            }
            let line_idx = raw_idx as usize;
            let (range, layout) = &self.visual_lines[line_idx];
            if range.start >= range.end {
                // Empty line
                return range.start;
            }

            let x = (position.x - bounds.left()).max(px(0.0));
            let line_slice = &self.content[range.clone()];
            let char_idx = layout.closest_index_for_x(x).min(line_slice.len());
            let mut final_idx = (range.start + char_idx).min(self.content.len());
            while final_idx > 0 && !self.content.is_char_boundary(final_idx) {
                final_idx -= 1;
            }
            return final_idx;
        }

        let lines: Vec<&str> = self.content.split(NL_CH).collect();
        if lines.is_empty() {
            return 0;
        }
        let total_lines = lines.len();
        let line_h = self.line_height.unwrap_or_else(|| {
            (bounds.bottom() - bounds.top()).max(px(1.0)) / total_lines as f32
        });
        let lines_h = line_h * total_lines as f32;

        if position.y >= bounds.top() + lines_h || position.y >= bounds.bottom() {
            return self.content.len();
        }

        let raw_line = if line_h > px(0.0) {
            ((position.y - bounds.top()) / line_h).floor() as isize
        } else {
            0
        };
        if raw_line < 0 {
            return 0;
        }
        if raw_line as usize >= total_lines {
            return self.content.len();
        }
        let line_idx = raw_line as usize;

        let mut offset = 0;
        for l in lines.iter().take(line_idx) {
            offset += l.len() + 1;
        }
        let line_str = lines[line_idx];
        if line_str.is_empty() {
            return offset;
        }
        if let Some(layout) = self.last_layouts.get(line_idx) {
            let x = (position.x - bounds.left()).max(px(0.0));
            let char_idx = layout.closest_index_for_x(x).min(line_str.len());
            let mut final_idx = (offset + char_idx).min(self.content.len());
            while final_idx > 0 && !self.content.is_char_boundary(final_idx) {
                final_idx -= 1;
            }
            final_idx
        } else {
            let char_w = px(7.5);
            let col = ((position.x - bounds.left()).max(px(0.0)) / char_w).round() as usize;
            let col = col.min(line_str.chars().count());
            let mut char_bytes = 0;
            for ch in line_str.chars().take(col) {
                char_bytes += ch.len_utf8();
            }
            let mut final_idx = (offset + char_bytes).min(self.content.len());
            while final_idx > 0 && !self.content.is_char_boundary(final_idx) {
                final_idx -= 1;
            }
            final_idx
        }
    }

    fn cursor(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_cursor(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selected_range = offset..offset;
        self.selection_reversed = false;
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
        if !self.visual_lines.is_empty() {
            for (range, _) in &self.visual_lines {
                if offset >= range.start && offset <= range.end {
                    return range.start;
                }
            }
        }
        line_start_of(&self.content, offset)
    }

    fn line_end(&self, offset: usize) -> usize {
        if !self.visual_lines.is_empty() {
            for (range, _) in &self.visual_lines {
                if offset >= range.start && offset <= range.end {
                    return range.end;
                }
            }
        }
        line_end_of(&self.content, offset)
    }

    fn prev_line_start(&self, offset: usize) -> usize {
        if !self.visual_lines.is_empty() {
            for (idx, (range, _)) in self.visual_lines.iter().enumerate() {
                if offset >= range.start && offset <= range.end {
                    if idx > 0 {
                        let rel = offset - range.start;
                        let prev_range = &self.visual_lines[idx - 1].0;
                        return (prev_range.start + rel).min(prev_range.end);
                    } else {
                        return 0;
                    }
                }
            }
        }
        let cur = line_start_of(&self.content, offset);
        if cur == 0 {
            0
        } else {
            line_start_of(&self.content, cur - 1)
        }
    }

    fn next_line_start(&self, offset: usize) -> usize {
        if !self.visual_lines.is_empty() {
            for (idx, (range, _)) in self.visual_lines.iter().enumerate() {
                if offset >= range.start && offset <= range.end {
                    if idx + 1 < self.visual_lines.len() {
                        let rel = offset - range.start;
                        let next_range = &self.visual_lines[idx + 1].0;
                        return (next_range.start + rel).min(next_range.end);
                    } else {
                        return self.content.len();
                    }
                }
            }
        }
        let end = line_end_of(&self.content, offset);
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
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        let s = range.start.min(self.content.len());
        let e = range.end.min(self.content.len());
        actual_range.replace(self.range_to_utf16(&(s..e)));
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

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.marked_range.take() {
            let start = range.start.min(self.content.len());
            let end = range.end.min(self.content.len());
            if end > start {
                self.content = format!("{}{}", &self.content[..start], &self.content[end..]);
                self.selected_range = start..start;
                self.selection_reversed = false;
                cx.emit(TextAreaEvent::Change(self.content.clone()));
                cx.notify();
            }
        }
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
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.take())
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
        self.selection_reversed = false;
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
        if self.read_only {
            return;
        }
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
        self.marked_range = Some(start..start + new_text.len());
        let cursor = start + new_text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.reset_blink(cx);
        cx.emit(TextAreaEvent::Change(self.content.clone()));
        cx.notify();
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
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let offset = range.start.min(self.content.len());
        let line_height = self.line_height.unwrap_or(px(20.0));

        if !self.visual_lines.is_empty() {
            for (idx, (v_range, layout)) in self.visual_lines.iter().enumerate() {
                if (offset >= v_range.start && offset <= v_range.end) || idx == self.visual_lines.len() - 1 {
                    let rel_offset = offset.saturating_sub(v_range.start).min(v_range.len());
                    let top = bounds.top() + line_height * idx as f32;
                    let x = layout.x_for_index(rel_offset);
                    return Some(Bounds::new(
                        point(bounds.left() + x, top),
                        size(px(2.0), line_height),
                    ));
                }
            }
        }

        let lines: Vec<&str> = self.content.split(NL_CH).collect();
        let mut cur_offset = 0;
        let mut target_line_idx = 0;
        let mut line_char_offset = 0;
        for (idx, line) in lines.iter().enumerate() {
            let next_offset = cur_offset + line.len() + 1;
            if offset <= cur_offset + line.len() || idx == lines.len() - 1 {
                target_line_idx = idx;
                line_char_offset = offset.saturating_sub(cur_offset);
                break;
            }
            cur_offset = next_offset;
        }
        let top = bounds.top() + line_height * target_line_idx as f32;
        let x = if let Some(layout) = self.last_layouts.get(target_line_idx) {
            layout.x_for_index(line_char_offset)
        } else {
            px(line_char_offset as f32 * 8.0)
        };
        Some(Bounds::new(
            point(bounds.left() + x, top),
            size(px(2.0), line_height),
        ))
    }
}

impl gpui::Render for TextArea {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let is_focused = self.focus_handle.is_focused(window);
        div()
            .id(("text_area_field", entity.entity_id()))
            .key_context("TextArea")
            .track_focus(&self.focus_handle)
            .cursor_text()
            .w_full()
            .h_full()
            .min_h(relative(1.))
            .p(px(10.0))
            .when(!self.borderless, |d| {
                d.rounded(px(6.0)).border_1().border_color(if is_focused {
                    rgba(0x3b82f6cc)
                } else {
                    rgba(0x00000000)
                })
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
                let is_left_down = event.pressed_button == Some(MouseButton::Left);
                this.on_mouse_move(event.position, is_left_down, cx);
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
            .child(TextAreaElement { input: entity })
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
    lines: Vec<ShapedLine>,
    visual_lines: Vec<(Range<usize>, ShapedLine)>,
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
        let line_count = if !ta.visual_lines.is_empty() {
            ta.visual_lines.len()
        } else {
            ta.content.split(NL_CH).count().max(1)
        };
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let line_height = window.line_height();
        let total_h = line_height * line_count as f32 + px(16.0);
        style.min_size.height = relative(1.).into();
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
        let marked_range = ta.marked_range.clone();
        let cursor_visible = ta.cursor_visible && is_focused && !ta.read_only;
        let cursor = ta.cursor();
        let soft_wrap = ta.soft_wrap;

        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();

        let mut visual_lines = Vec::new();

        let all_lines: Vec<String> = if content.is_empty() {
            vec![placeholder.clone()]
        } else {
            content.split(NL_CH).map(String::from).collect()
        };

        let wrap_width = (bounds.size.width - px(4.0)).max(px(50.0));

        let mut byte_offset = 0usize;
        for line_text in all_lines.iter() {
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
            } else if let Some(ref marked) = marked_range {
                let m_start = marked.start.max(line_start).min(line_end);
                let m_end = marked.end.max(line_start).min(line_end);
                if m_end > m_start {
                    let base_run = TextRun {
                        len: 0,
                        font: style.font(),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let pre_len = m_start - line_start;
                    let marked_len = m_end - m_start;
                    let post_len = line_end - m_end;
                    let mut r = Vec::new();
                    if pre_len > 0 {
                        r.push(TextRun {
                            len: pre_len,
                            ..base_run.clone()
                        });
                    }
                    if marked_len > 0 {
                        r.push(TextRun {
                            len: marked_len,
                            underline: Some(UnderlineStyle {
                                color: Some(color),
                                thickness: px(1.0),
                                wavy: false,
                            }),
                            ..base_run.clone()
                        });
                    }
                    if post_len > 0 {
                        r.push(TextRun {
                            len: post_len,
                            ..base_run
                        });
                    }
                    r
                } else {
                    vec![TextRun {
                        len: line_text.len(),
                        font: style.font(),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }]
                }
            } else {
                vec![TextRun {
                    len: line_text.len(),
                    font: style.font(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }]
            };

            if soft_wrap && !is_placeholder && wrap_width > px(0.0) {
                let wrapped = wrap_logical_line(
                    line_text,
                    line_start,
                    wrap_width,
                    font_size,
                    &runs,
                    window,
                );
                visual_lines.extend(wrapped);
            } else {
                let shaped = window.text_system().shape_line(
                    SharedString::from(line_text.clone()),
                    font_size,
                    if runs.is_empty() { &[] as &[TextRun] } else { &runs },
                    None,
                );
                visual_lines.push((line_start..line_end, shaped));
            }
        }

        let mut lines = Vec::with_capacity(visual_lines.len());
        let mut selections = Vec::new();
        let mut cursor_quad = None;
        let total_visual = visual_lines.len();

        for (v_idx, (v_range, shaped)) in visual_lines.iter().enumerate() {
            let top = bounds.top() + line_height * v_idx as f32;

            // Paint text selection only when NOT actively composing IME marked text
            if !content.is_empty() && selected.end > selected.start && marked_range.is_none() {
                let sel_start = selected.start.max(v_range.start).min(v_range.end);
                let sel_end = selected.end.max(v_range.start).min(v_range.end);
                if sel_end > sel_start {
                    let x0 = shaped.x_for_index(sel_start - v_range.start);
                    let x1 = shaped.x_for_index(sel_end - v_range.start);
                    selections.push(fill(
                        Bounds::from_corners(
                            point(bounds.left() + x0, top),
                            point(bounds.left() + x1, top + line_height),
                        ),
                        rgba(0x3b82f633),
                    ));
                }
            }

            // Cursor placement
            if cursor_visible && cursor_quad.is_none() {
                let is_last_visual = v_idx + 1 == total_visual;
                let is_empty_line = v_range.start == v_range.end;
                let is_logical_end = is_last_visual
                    || (v_range.end < content.len()
                        && (content.as_bytes().get(v_range.end) == Some(&b'\n')
                            || content.as_bytes().get(v_range.end) == Some(&b'\r')));

                let matches = if is_empty_line {
                    cursor == v_range.start
                } else if is_logical_end {
                    cursor >= v_range.start && cursor <= v_range.end
                } else {
                    cursor >= v_range.start && cursor < v_range.end
                };

                if matches {
                    let rel_cursor = (cursor - v_range.start).min(v_range.len());
                    let cx_pos = shaped.x_for_index(rel_cursor);
                    cursor_quad = Some(fill(
                        Bounds::new(
                            point(bounds.left() + cx_pos, top + px(1.0)),
                            size(px(1.5), line_height - px(2.0)),
                        ),
                        style.color,
                    ));
                }
            }

            lines.push(shaped.clone());
        }

        // Defensive fallback: if cursor is visible but no visual line matched,
        // place it at the nearest boundary on the last visual line so it never vanishes.
        if cursor_visible && cursor_quad.is_none() {
            if let Some((v_range, shaped)) = visual_lines.last() {
                let top = bounds.top()
                    + line_height * (visual_lines.len().saturating_sub(1)) as f32;
                let rel = cursor.saturating_sub(v_range.start).min(v_range.len());
                let cx_pos = shaped.x_for_index(rel);
                cursor_quad = Some(fill(
                    Bounds::new(
                        point(bounds.left() + cx_pos, top + px(1.0)),
                        size(px(1.5), line_height - px(2.0)),
                    ),
                    style.color,
                ));
            }
        }

        TextAreaPrepaint {
            lines,
            visual_lines,
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
        for (idx, line) in prepaint.lines.iter().enumerate() {
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

        let painted_lines = std::mem::take(&mut prepaint.lines);
        let visual_lines = std::mem::take(&mut prepaint.visual_lines);
        self.input.update(cx, |input, _| {
            input.last_bounds = Some(bounds);
            input.line_height = Some(line_height);
            input.last_layouts = painted_lines;
            input.visual_lines = visual_lines;
        });
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

    #[test]
    fn test_multiline_split_and_line_count() {
        let empty = "";
        assert_eq!(empty.split(NL_CH).count().max(1), 1);

        let one_line = "hello";
        assert_eq!(one_line.split(NL_CH).count(), 1);

        let trailing_newline = "hello\n";
        assert_eq!(trailing_newline.split(NL_CH).count(), 2);

        let two_empty_lines = "\n\n";
        assert_eq!(two_empty_lines.split(NL_CH).count(), 3);
    }

    #[test]
    fn test_word_bounds_selection() {
        let text = "hello world\nmultiline text";
        assert_eq!(word_bounds_at(text, 2), 0..5);
        assert_eq!(word_bounds_at(text, 14), 12..21);
    }

    #[test]
    fn test_cursor_matching_logic() {
        let content = "hello\n\nworld";
        // Line 0: "hello", range 0..5, logical end followed by \n at byte 5
        // Line 1: "", range 6..6, empty line followed by \n at byte 6
        // Line 2: "world", range 7..12, last visual line
        let check_matches = |cursor: usize, v_idx: usize, total_visual: usize, v_range: std::ops::Range<usize>| -> bool {
            let is_last_visual = v_idx + 1 == total_visual;
            let is_empty_line = v_range.start == v_range.end;
            let is_logical_end = is_last_visual
                || (v_range.end < content.len()
                    && (content.as_bytes().get(v_range.end) == Some(&b'\n')
                        || content.as_bytes().get(v_range.end) == Some(&b'\r')));

            if is_empty_line {
                cursor == v_range.start
            } else if is_logical_end {
                cursor >= v_range.start && cursor <= v_range.end
            } else {
                cursor >= v_range.start && cursor < v_range.end
            }
        };

        // Cursor at 5 (end of line 0) matches line 0
        assert!(check_matches(5, 0, 3, 0..5));
        assert!(!check_matches(5, 1, 3, 6..6));
        assert!(!check_matches(5, 2, 3, 7..12));

        // Cursor at 6 (on empty line 1) matches line 1
        assert!(!check_matches(6, 0, 3, 0..5));
        assert!(check_matches(6, 1, 3, 6..6));
        assert!(!check_matches(6, 2, 3, 7..12));

        // Cursor at 12 (end of line 2) matches line 2
        assert!(!check_matches(12, 0, 3, 0..5));
        assert!(!check_matches(12, 1, 3, 6..6));
        assert!(check_matches(12, 2, 3, 7..12));
    }
}
