use gpui::{
    div, px, rgba, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, Render, Styled, Window, prelude::*,
};
use gpui_kit::component::input::{Input, InputEvent, InputState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
    Change(String),
    Escape,
    Enter,
    Up,
    Down,
}

pub struct TextInput {
    pub focus_handle: FocusHandle,
    pub initial_focus_handle: FocusHandle,
    pub content: String,
    pub placeholder: String,
    pub is_secret: bool,
    pub read_only: bool,
    pub borderless: bool,
    pub cleanable: bool,
    pub select_all_on_render: bool,
    pub state: Option<Entity<InputState>>,
    _subscription: Option<gpui::Subscription>,
    _keystroke_sub: Option<gpui::Subscription>,
}

impl gpui::EventEmitter<TextInputEvent> for TextInput {}

impl TextInput {
    pub fn new(placeholder: impl Into<String>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        Self {
            focus_handle: focus_handle.clone(),
            initial_focus_handle: focus_handle,
            content: String::new(),
            placeholder: placeholder.into(),
            is_secret: false,
            read_only: false,
            borderless: false,
            cleanable: false,
            select_all_on_render: false,
            state: None,
            _subscription: None,
            _keystroke_sub: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn is_blinking(&self) -> bool {
        true
    }

    pub fn start_blink(&mut self, _cx: &mut Context<Self>) {}

    pub fn set_secret(&mut self, is_secret: bool, cx: &mut Context<Self>) {
        self.is_secret = is_secret;
        cx.notify();
    }

    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.read_only = read_only;
        if let Some(state) = &self.state {
            state.update(cx, |s, cx| s.set_readonly(read_only, cx));
        }
        cx.notify();
    }

    pub fn set_borderless(&mut self, borderless: bool) {
        self.borderless = borderless;
    }

    pub fn set_cleanable(&mut self, cleanable: bool) {
        self.cleanable = cleanable;
    }

    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content = text.into();
        cx.emit(TextInputEvent::Change(self.content.clone()));
        cx.notify();
    }

    pub fn set_text_silent(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content = text.into();
        cx.notify();
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>, cx: &mut Context<Self>) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.select_all_on_render = true;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content.clear();
        cx.emit(TextInputEvent::Change(String::new()));
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if let Some(state) = &self.state {
            state.read(cx).presentation().focus_handle().focus(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = match &self.state {
            Some(state) => {
                if state.read(cx).value() != self.content.as_str() {
                    state.update(cx, |s, cx| {
                        s.set_value(self.content.clone(), window, cx);
                    });
                }
                let pres = state.read(cx).presentation();
                if pres.placeholder().as_str() != self.placeholder.as_str() {
                    state.update(cx, |s, cx| {
                        s.set_placeholder(self.placeholder.clone(), window, cx);
                    });
                }
                if pres.is_masked() != self.is_secret {
                    state.update(cx, |s, cx| {
                        s.set_masked(self.is_secret, window, cx);
                    });
                }
                if pres.is_readonly() != self.read_only {
                    state.update(cx, |s, cx| {
                        s.set_readonly(self.read_only, cx);
                    });
                }
                if self.select_all_on_render {
                    self.select_all_on_render = false;
                    state.update(cx, |s, cx| {
                        s.select_all(window, cx);
                    });
                }
                state.clone()
            }
            None => {
                let content = self.content.clone();
                let placeholder = self.placeholder.clone();
                let is_secret = self.is_secret;
                let read_only = self.read_only;

                let state = cx.new(|cx| {
                    let mut s = InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(content)
                        .masked(is_secret);
                    s.set_readonly(read_only, cx);
                    s
                });

                if self.select_all_on_render {
                    self.select_all_on_render = false;
                    state.update(cx, |s, cx| {
                        s.select_all(window, cx);
                    });
                }

                let sub = cx.subscribe(&state, move |this, state, event: &InputEvent, cx| {
                    match event {
                        InputEvent::Change => {
                            let val = state.read(cx).value().to_string();
                            this.content = val.clone();
                            cx.emit(TextInputEvent::Change(val));
                        }
                        InputEvent::PressEnter { .. } => {
                            this.content = state.read(cx).value().to_string();
                            cx.emit(TextInputEvent::Enter);
                        }
                        _ => {}
                    }
                });

                let entity = cx.entity().clone();
                let focus_handle = state.read(cx).presentation().focus_handle().clone();
                let key_sub = cx.intercept_keystrokes({
                    let focus_handle = focus_handle.clone();
                    let entity = entity.clone();
                    move |event, window, cx| {
                        if focus_handle.is_focused(window) {
                            let key = event.keystroke.key.as_str();
                            if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
                                if key.eq_ignore_ascii_case("p") {
                                    entity.update(cx, |_, cx| cx.emit(TextInputEvent::Up));
                                    return;
                                } else if key.eq_ignore_ascii_case("n") {
                                    entity.update(cx, |_, cx| cx.emit(TextInputEvent::Down));
                                    return;
                                }
                            }
                            match key {
                                "up" => {
                                    entity.update(cx, |_, cx| cx.emit(TextInputEvent::Up));
                                }
                                "down" => {
                                    entity.update(cx, |_, cx| cx.emit(TextInputEvent::Down));
                                }
                                "escape" => {
                                    entity.update(cx, |_, cx| cx.emit(TextInputEvent::Escape));
                                }
                                _ => {}
                            }
                        }
                    }
                });

                if self.initial_focus_handle.is_focused(window) {
                    state.update(cx, |s, cx| s.focus(window, cx));
                }

                self.focus_handle = focus_handle;
                self._subscription = Some(sub);
                self._keystroke_sub = Some(key_sub);
                self.state = Some(state.clone());

                state
            }
        };

        let is_focused = state.read(cx).presentation().focus_handle().is_focused(window);

        div()
            .id(("text_input_wrapper", cx.entity_id()))
            .size_full()
            .flex()
            .items_center()
            .when(!self.borderless && is_focused, |d| {
                d.rounded(px(6.0)).border_1().border_color(rgba(0x3b82f6cc))
            })
            .child(
                Input::new(&state)
                    .appearance(false)
                    .bordered(false)
                    .cleanable(self.cleanable)
                    .w_full()
                    .h_full(),
            )
    }
}
