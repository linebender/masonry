
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use tracing::{error, info, info_span};

// Automatically defaults to std::time::Instant on non Wasm platforms
use instant::Instant;

use crate::kurbo::{Point, Size};
use crate::piet::{Color, Piet, RenderContext};

use crate::ext_event::{ExtEventHost, ExtEventSink};
use crate::{
    Command, Data, Env, Event, Handled, InternalEvent, KeyEvent, PlatformError, Selector, Target,
    TimerToken, WidgetId, WindowDesc, WindowId,
    BoxConstraints, EventCtx,
    InternalLifeCycle, LayoutCtx, LifeCycle, LifeCycleCtx, PaintCtx,
    UpdateCtx, Widget, WidgetPod,
};
use crate::text::TextFieldRegistration;
use crate::util::ExtendDrain;
use crate::widget::LabelText;
use crate::contexts::ContextState;
use crate::core::{CommandQueue, FocusChange, WidgetState};

use crate::window_handling::win_handler::{DialogInfo, EXT_EVENT_IDLE_TOKEN};
use crate::window_handling::window_description::{WindowConfig, PendingWindow, WindowSizePolicy};
use crate::window_handling::win_handler::RUN_COMMANDS_TOKEN;
use crate::command::sys as sys_cmd;

use druid_shell::WindowBuilder;
use druid_shell::{
    Cursor, text::InputHandler, Application, FileDialogToken, FileInfo, IdleToken, MouseEvent, Region,
    Scale, TextFieldToken, WinHandler, WindowHandle,
};


pub(crate) struct AppRoot {
    pub app: Application,
    pub command_queue: CommandQueue,
    pub file_dialogs: HashMap<FileDialogToken, DialogInfo>,
    pub ext_event_host: ExtEventHost,
    pub windows: Windows,
    /// The id of the most-recently-focused window that has a menu. On macOS, this
    /// is the window that's currently in charge of the app menu.
    #[allow(unused)]
    pub menu_window: Option<WindowId>,
    pub(crate) env: Env,
    pub ime_focus_change: Option<Box<dyn Fn()>>,
}

// TODO - remove
/// All active windows.
#[derive(Default)]
pub struct Windows {
    pub pending: HashMap<WindowId, PendingWindow>,
    pub active_windows: HashMap<WindowId, WindowRoot>,
}

pub type ImeUpdateFn = dyn FnOnce(druid_shell::text::Event);

/// Per-window state not owned by user code.
pub struct WindowRoot {
    pub(crate) id: WindowId,
    pub(crate) root: WidgetPod<Box<dyn Widget>>,
    pub(crate) title: LabelText,
    size_policy: WindowSizePolicy,
    size: Size,
    invalid: Region,
    // This will be `Some` whenever the most recently displayed frame was an animation frame.
    pub(crate) last_anim: Option<Instant>,
    pub(crate) last_mouse_pos: Option<Point>,
    pub(crate) focus: Option<WidgetId>,
    pub(crate) handle: WindowHandle,
    pub(crate) timers: HashMap<TimerToken, WidgetId>,
    pub(crate) transparent: bool,
    pub(crate) ime_handlers: Vec<(TextFieldToken, TextFieldRegistration)>,
    ext_handle: ExtEventSink,
    pub(crate) ime_focus_change: Option<Option<TextFieldToken>>,
}

// ---

impl Windows {
    pub fn connect(&mut self, id: WindowId, handle: WindowHandle, ext_handle: ExtEventSink) {
        if let Some(pending) = self.pending.remove(&id) {
            let win = WindowRoot::new(id, handle, pending, ext_handle);
            assert!(self.active_windows.insert(id, win).is_none(), "duplicate window");
        } else {
            tracing::error!("no window for connecting handle {:?}", id);
        }
    }

    pub fn add(&mut self, id: WindowId, win: PendingWindow) {
        assert!(self.pending.insert(id, win).is_none(), "duplicate pending");
    }

    pub fn count(&self) -> usize {
        self.active_windows.len() + self.pending.len()
    }
}


impl AppRoot {
    pub fn append_command(&mut self, cmd: Command) {
        self.command_queue.push_back(cmd);
    }

    pub fn connect(&mut self, id: WindowId, handle: WindowHandle) {
        self.windows
            .connect(id, handle, self.ext_event_host.make_sink());

        // If the external event host has no handle, it cannot wake us
        // when an event arrives.
        if self.ext_event_host.handle_window_id.is_none() {
            self.set_ext_event_idle_handler(id);
        }
    }

    /// Called after this window has been closed by the platform.
    ///
    /// We clean up resources.
    pub fn remove_window(&mut self, window_id: WindowId) {
        // when closing the last window:
        if let Some(mut win) = self.windows.active_windows.remove(&window_id) {
            if self.windows.active_windows.is_empty() {
                // If there are even no pending windows, we quit the run loop.
                if self.windows.count() == 0 {
                    #[cfg(any(target_os = "windows", feature = "x11"))]
                    self.app.quit();
                }
            }
        }

        // if we are closing the window that is currently responsible for
        // waking us when external events arrive, we want to pass that responsibility
        // to another window.
        if self.ext_event_host.handle_window_id == Some(window_id) {
            self.ext_event_host.handle_window_id = None;
            // find any other live window
            let win_id = self.windows.active_windows.keys().find(|k| *k != &window_id);
            if let Some(any_other_window) = win_id.cloned() {
                self.set_ext_event_idle_handler(any_other_window);
            }
        }
    }

    /// Set the idle handle that will be used to wake us when external events arrive.
    pub fn set_ext_event_idle_handler(&mut self, id: WindowId) {
        if let Some(mut idle) = self
            .windows
            .active_windows.get_mut(&id)
            .and_then(|win| win.handle.get_idle_handle())
        {
            if self.ext_event_host.has_pending_items() {
                idle.schedule_idle(EXT_EVENT_IDLE_TOKEN);
            }
            self.ext_event_host.set_idle(idle, id);
        }
    }

    /// triggered by a menu item or other command.
    ///
    /// This doesn't close the window; it calls the close method on the platform
    /// window handle; the platform should close the window, and then call
    /// our handlers `destroy()` method, at which point we can do our cleanup.
    pub fn request_close_window(&mut self, window_id: WindowId) {
        if let Some(win) = self.windows.active_windows.get_mut(&window_id) {
            win.handle.close();
        }
    }

    /// Requests the platform to close all windows.
    pub fn request_close_all_windows(&mut self) {
        for win in self.windows.active_windows.values_mut() {
            win.handle.close();
        }
    }

    pub fn show_window(&mut self, id: WindowId) {
        if let Some(win) = self.windows.active_windows.get_mut(&id) {
            win.handle.bring_to_front_and_focus();
        }
    }

    pub fn configure_window(&mut self, config: &WindowConfig, id: WindowId) {
        if let Some(win) = self.windows.active_windows.get_mut(&id) {
            config.apply_to_handle(&mut win.handle);
        }
    }

    pub fn prepare_paint(&mut self, window_id: WindowId) {
        if let Some(win) = self.windows.active_windows.get_mut(&window_id) {
            win.prepare_paint(&mut self.command_queue, &self.env);
        }
        //self.do_update();
        self.invalidate_and_finalize();
    }

    pub fn paint(&mut self, window_id: WindowId, piet: &mut Piet, invalid: &Region) {
        if let Some(win) = self.windows.active_windows.get_mut(&window_id) {
            win.do_paint(
                piet,
                invalid,
                &mut self.command_queue,
                &self.env,
            );
        }
    }

    pub fn dispatch_cmd(&mut self, cmd: Command) -> Handled {
        self.invalidate_and_finalize();

        match cmd.target() {
            Target::Window(id) => {
                if let Some(w) = self.windows.active_windows.get_mut(&id) {
                    return if cmd.is(sys_cmd::CLOSE_WINDOW) {
                        let handled = w.event(
                            &mut self.command_queue,
                            Event::WindowCloseRequested,
                            &self.env,
                        );
                        if !handled.is_handled() {
                            w.event(
                                &mut self.command_queue,
                                Event::WindowDisconnected,
                                    &self.env,
                            );
                        }
                        handled
                    } else {
                        w.event(
                            &mut self.command_queue,
                            Event::Command(cmd),
                            &self.env,
                        )
                    };
                }
            }
            // in this case we send it to every window that might contain
            // this widget, breaking if the event is handled.
            Target::Widget(id) => {
                for w in self.windows.active_windows.values_mut().filter(|w| w.may_contain_widget(id)) {
                    let event = Event::Internal(InternalEvent::TargetedCommand(cmd.clone()));
                    if w.event(&mut self.command_queue, event, &self.env)
                        .is_handled()
                    {
                        return Handled::Yes;
                    }
                }
            }
            Target::Global => {
                for w in self.windows.active_windows.values_mut() {
                    let event = Event::Command(cmd.clone());
                    if w.event(&mut self.command_queue, event, &self.env)
                        .is_handled()
                    {
                        return Handled::Yes;
                    }
                }
            }
            Target::Auto => {
                tracing::error!("{:?} reached window handler with `Target::Auto`", cmd);
            }
        }
        Handled::No
    }

    pub fn do_window_event(&mut self, source_id: WindowId, event: Event) -> Handled {
        match event {
            Event::Command(..) | Event::Internal(InternalEvent::TargetedCommand(..)) => {
                panic!("commands should be dispatched via dispatch_cmd");
            }
            _ => (),
        }

        if let Some(win) = self.windows.active_windows.get_mut(&source_id) {
            win.event(&mut self.command_queue, event, &self.env)
        } else {
            Handled::No
        }
    }

    pub fn do_update(&mut self) {
        /*
        // we send `update` to all windows, not just the active one:
        for window in self.windows.active_windows.values_mut() {
            window.update(&mut self.command_queue, &self.env);
            if let Some(focus_change) = window.ime_focus_change.take() {
                // we need to call this outside of the borrow, so we create a
                // closure that takes the correct window handle. yes, it feels
                // weird.
                let handle = window.handle.clone();
                let f = Box::new(move || handle.set_focused_text_field(focus_change));
                self.ime_focus_change = Some(f);
            }
        }
        */
        self.invalidate_and_finalize();
    }

    /// invalidate any window handles that need it.
    ///
    /// This should always be called at the end of an event update cycle,
    /// including for lifecycle events.
    pub fn invalidate_and_finalize(&mut self) {
        for win in self.windows.active_windows.values_mut() {
            win.invalidate_and_finalize();
        }
    }

    pub fn ime_update_fn(&self, window_id: WindowId, widget_id: WidgetId) -> Option<Box<ImeUpdateFn>> {
        self.windows
            .active_windows.get(&window_id)
            .and_then(|window| window.ime_invalidation_fn(widget_id))
    }

    pub fn get_ime_lock(
        &mut self,
        window_id: WindowId,
        token: TextFieldToken,
        mutable: bool,
    ) -> Box<dyn InputHandler> {
        self.windows
            .active_windows.get_mut(&window_id)
            .unwrap()
            .get_ime_handler(token, mutable)
    }

    /// Returns a `WidgetId` if the lock was mutable; the widget should be updated.
    pub fn release_ime_lock(&mut self, window_id: WindowId, token: TextFieldToken) -> Option<WidgetId> {
        self.windows
            .active_windows.get_mut(&window_id)
            .unwrap()
            .release_ime_lock(token)
    }

    pub fn window_got_focus(&mut self, _window_id: WindowId) {
        // TODO
    }
}

// ---

impl WindowRoot {
    pub(crate) fn new(
        id: WindowId,
        handle: WindowHandle,
        pending: PendingWindow,
        ext_handle: ExtEventSink,
    ) -> WindowRoot {
        WindowRoot {
            id,
            root: WidgetPod::new(pending.root),
            size_policy: pending.size_policy,
            size: Size::ZERO,
            invalid: Region::EMPTY,
            title: pending.title,
            transparent: pending.transparent,
            last_anim: None,
            last_mouse_pos: None,
            focus: None,
            handle,
            timers: HashMap::new(),
            ext_handle,
            ime_handlers: Vec::new(),
            ime_focus_change: None,
        }
    }

    /// `true` iff any child requested an animation frame since the last `AnimFrame` event.
    pub(crate) fn wants_animation_frame(&self) -> bool {
        self.root.state().request_anim
    }

    pub(crate) fn focus_chain(&self) -> &[WidgetId] {
        &self.root.state().focus_chain
    }

    /// Returns `true` if the provided widget may be in this window,
    /// but it may also be a false positive.
    /// However when this returns `false` the widget is definitely not in this window.
    pub(crate) fn may_contain_widget(&self, widget_id: WidgetId) -> bool {
        // The bloom filter we're checking can return false positives.
        widget_id == self.root.id() || self.root.state().children.may_contain(&widget_id)
    }

    fn post_event_processing(
        &mut self,
        widget_state: &mut WidgetState,
        queue: &mut CommandQueue,
        env: &Env,
        process_commands: bool,
    ) {
        // If children are changed during the handling of an event,
        // we need to send RouteWidgetAdded now, so that they are ready for update/layout.
        if widget_state.children_changed {
            // Anytime widgets are removed we check and see if any of those
            // widgets had IME sessions and unregister them if so.
            let WindowRoot {
                ime_handlers,
                handle,
                ..
            } = self;
            ime_handlers.retain(|(token, v)| {
                let will_retain = v.is_alive();
                if !will_retain {
                    tracing::debug!("{:?} removed", token);
                    handle.remove_text_field(*token);
                }
                will_retain
            });

            self.lifecycle(
                queue,
                &LifeCycle::Internal(InternalLifeCycle::RouteWidgetAdded),
                env,
                false,
            );
        }

        if self.root.state().needs_window_origin && !self.root.state().needs_layout {
            let event = LifeCycle::Internal(InternalLifeCycle::ParentWindowOrigin);
            self.lifecycle(queue, &event, env, false);
        }

        // Update the disabled state if necessary
        // Always do this before updating the focus-chain
        if self.root.state().tree_disabled_changed() {
            let event = LifeCycle::Internal(InternalLifeCycle::RouteDisabledChanged);
            self.lifecycle(queue, &event, env, false);
        }

        // Update the focus-chain if necessary
        // Always do this before sending focus change, since this event updates the focus chain.
        if self.root.state().update_focus_chain {
            let event = LifeCycle::BuildFocusChain;
            self.lifecycle(queue, &event, env, false);
        }

        self.update_focus(widget_state, queue, env);

        // Add all the requested timers to the window's timers map.
        self.timers.extend_drain(&mut widget_state.timers);

        // If we need a new paint pass, make sure druid-shell knows it.
        if self.wants_animation_frame() {
            self.handle.request_anim_frame();
        }
        self.invalid.union_with(&widget_state.invalid);
        for ime_field in widget_state.text_registrations.drain(..) {
            let token = self.handle.add_text_field();
            tracing::debug!("{:?} added", token);
            self.ime_handlers.push((token, ime_field));
        }

        // If there are any commands and they should be processed
        if process_commands && !queue.is_empty() {
            // Ask the handler to call us back on idle
            // so we can process them in a new event/update pass.
            if let Some(mut handle) = self.handle.get_idle_handle() {
                handle.schedule_idle(RUN_COMMANDS_TOKEN);
            } else {
                error!("failed to get idle handle");
            }
        }
    }

    pub(crate) fn event(
        &mut self,
        queue: &mut CommandQueue,
        event: Event,
        env: &Env,
    ) -> Handled {
        match &event {
            Event::WindowSize(size) => self.size = *size,
            Event::MouseDown(e) | Event::MouseUp(e) | Event::MouseMove(e) | Event::Wheel(e) => {
                self.last_mouse_pos = Some(e.pos)
            }
            Event::Internal(InternalEvent::MouseLeave) => self.last_mouse_pos = None,
            _ => (),
        }

        let event = match event {
            Event::Timer(token) => {
                if let Some(widget_id) = self.timers.get(&token) {
                    Event::Internal(InternalEvent::RouteTimer(token, *widget_id))
                } else {
                    error!("No widget found for timer {:?}", token);
                    return Handled::No;
                }
            }
            other => other,
        };

        if let Event::WindowConnected = event {
            self.lifecycle(
                queue,
                &LifeCycle::Internal(InternalLifeCycle::RouteWidgetAdded),
                env,
                false,
            );
        }

        let mut widget_state = WidgetState::new(self.root.id(), Some(self.size));
        let is_handled = {
            let mut state =
                ContextState::new(queue, &self.ext_handle, &self.handle, self.id, self.focus);
            let mut notifications = VecDeque::new();
            let mut ctx = EventCtx {
                state: &mut state,
                notifications: &mut notifications,
                widget_state: &mut widget_state,
                is_handled: false,
                is_root: true,
            };

            {
                let _span = info_span!("event");
                let _span = _span.enter();
                self.root.on_event(&mut ctx, &event, env);
            }

            if !ctx.notifications.is_empty() {
                info!("{} unhandled notifications:", ctx.notifications.len());
                for (i, n) in ctx.notifications.iter().enumerate() {
                    info!("{}: {:?}", i, n);
                }
            }
            Handled::from(ctx.is_handled)
        };

        // Clean up the timer token and do it immediately after the event handling
        // because the token may be reused and re-added in a lifecycle pass below.
        if let Event::Internal(InternalEvent::RouteTimer(token, _)) = event {
            self.timers.remove(&token);
        }

        if let Some(cursor) = &widget_state.cursor {
            self.handle.set_cursor(cursor);
        } else if matches!(
            event,
            Event::MouseMove(..) | Event::Internal(InternalEvent::MouseLeave)
        ) {
            self.handle.set_cursor(&Cursor::Arrow);
        }

        if matches!(
            (event, self.size_policy),
            (Event::WindowSize(_), WindowSizePolicy::Content)
        ) {
            // Because our initial size can be zero, the window system won't ask us to paint.
            // So layout ourselves and hopefully we resize
            self.layout(queue, env);
        }

        self.post_event_processing(&mut widget_state, queue, env, false);

        is_handled
    }

    pub(crate) fn lifecycle(
        &mut self,
        queue: &mut CommandQueue,
        event: &LifeCycle,
        env: &Env,
        process_commands: bool,
    ) {
        let mut widget_state = WidgetState::new(self.root.id(), Some(self.size));
        let mut state =
            ContextState::new(queue, &self.ext_handle, &self.handle, self.id, self.focus);
        let mut ctx = LifeCycleCtx {
            state: &mut state,
            widget_state: &mut widget_state,
        };

        {
            let _span = info_span!("lifecycle");
            let _span = _span.enter();
            self.root.lifecycle(&mut ctx, event, env);
        }

        self.post_event_processing(&mut widget_state, queue, env, process_commands);
    }

    pub(crate) fn invalidate_and_finalize(&mut self) {
        if self.root.state().needs_layout {
            self.handle.invalidate();
        } else {
            for rect in self.invalid.rects() {
                self.handle.invalidate_rect(*rect);
            }
        }
        self.invalid.clear();
    }

    #[allow(dead_code)]
    pub(crate) fn invalid(&self) -> &Region {
        &self.invalid
    }

    #[allow(dead_code)]
    pub(crate) fn invalid_mut(&mut self) -> &mut Region {
        &mut self.invalid
    }

    /// Get ready for painting, by doing layout and sending an `AnimFrame` event.
    pub(crate) fn prepare_paint(&mut self, queue: &mut CommandQueue, env: &Env) {
        let now = Instant::now();
        // TODO: this calculation uses wall-clock time of the paint call, which
        // potentially has jitter.
        //
        // See https://github.com/linebender/druid/issues/85 for discussion.
        let last = self.last_anim.take();
        let elapsed_ns = last.map(|t| now.duration_since(t).as_nanos()).unwrap_or(0) as u64;

        if self.wants_animation_frame() {
            self.event(queue, Event::AnimFrame(elapsed_ns), env);
            self.last_anim = Some(now);
        }
    }

    pub(crate) fn do_paint(
        &mut self,
        piet: &mut Piet,
        invalid: &Region,
        queue: &mut CommandQueue,
        env: &Env,
    ) {
        if self.root.state().needs_layout {
            self.layout(queue, env);
        }

        for &r in invalid.rects() {
            piet.clear(
                Some(r),
                if self.transparent {
                    Color::TRANSPARENT
                } else {
                    env.get(crate::theme::WINDOW_BACKGROUND_COLOR)
                },
            );
        }
        self.paint(piet, invalid, queue, env);
    }

    fn layout(&mut self, queue: &mut CommandQueue, env: &Env) {
        let mut widget_state = WidgetState::new(self.root.id(), Some(self.size));
        let mut state =
            ContextState::new(queue, &self.ext_handle, &self.handle, self.id, self.focus);
        let mut layout_ctx = LayoutCtx {
            state: &mut state,
            widget_state: &mut widget_state,
            mouse_pos: self.last_mouse_pos,
        };
        let bc = match self.size_policy {
            WindowSizePolicy::User => BoxConstraints::tight(self.size),
            WindowSizePolicy::Content => BoxConstraints::UNBOUNDED,
        };

        let content_size = {
            let _span = info_span!("layout");
            let _span = _span.enter();
            self.root.layout(&mut layout_ctx, &bc, env)
        };

        if let WindowSizePolicy::Content = self.size_policy {
            let insets = self.handle.content_insets();
            let full_size = (content_size.to_rect() + insets).size();
            if self.size != full_size {
                self.size = full_size;
                self.handle.set_size(full_size)
            }
        }
        self.root
            .set_origin(&mut layout_ctx, env, Point::ORIGIN);
        self.lifecycle(
            queue,
            &LifeCycle::Internal(InternalLifeCycle::ParentWindowOrigin),
            env,
            false,
        );
        self.post_event_processing(&mut widget_state, queue, env, true);
    }

    /// only expose `layout` for testing; normally it is called as part of `do_paint`
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn just_layout(&mut self, queue: &mut CommandQueue, env: &Env) {
        self.layout(queue, env)
    }

    fn paint(
        &mut self,
        piet: &mut Piet,
        invalid: &Region,
        queue: &mut CommandQueue,
        env: &Env,
    ) {
        let widget_state = WidgetState::new(self.root.id(), Some(self.size));
        let mut state =
            ContextState::new(queue, &self.ext_handle, &self.handle, self.id, self.focus);
        let mut ctx = PaintCtx {
            render_ctx: piet,
            state: &mut state,
            widget_state: &widget_state,
            z_ops: Vec::new(),
            region: invalid.clone(),
            depth: 0,
        };

        let root = &mut self.root;
        info_span!("paint").in_scope(|| {
            ctx.with_child_ctx(invalid.clone(), |ctx| root.paint_raw(ctx, env));
        });

        let mut z_ops = std::mem::take(&mut ctx.z_ops);
        z_ops.sort_by_key(|k| k.z_index);

        for z_op in z_ops.into_iter() {
            ctx.with_child_ctx(invalid.clone(), |ctx| {
                ctx.with_save(|ctx| {
                    ctx.render_ctx.transform(z_op.transform);
                    (z_op.paint_func)(ctx);
                });
            });
        }

        if self.wants_animation_frame() {
            self.handle.request_anim_frame();
        }
    }

    pub(crate) fn update_title(&mut self, env: &Env) {
        if self.title.resolve(env) {
            self.handle.set_title(&self.title.display_text());
        }
    }

    pub(crate) fn get_ime_handler(
        &mut self,
        req_token: TextFieldToken,
        mutable: bool,
    ) -> Box<dyn InputHandler> {
        self.ime_handlers
            .iter()
            .find(|(token, _)| req_token == *token)
            .and_then(|(_, reg)| reg.document.acquire(mutable))
            .unwrap()
    }

    fn update_focus(
        &mut self,
        widget_state: &mut WidgetState,
        queue: &mut CommandQueue,
        env: &Env,
    ) {
        if let Some(focus_req) = widget_state.request_focus.take() {
            let old = self.focus;
            let new = self.widget_for_focus_request(focus_req);
            // Only send RouteFocusChanged in case there's actual change
            if old != new {
                let event = LifeCycle::Internal(InternalLifeCycle::RouteFocusChanged { old, new });
                self.lifecycle(queue, &event, env, false);
                self.focus = new;
                // check if the newly focused widget has an IME session, and
                // notify the system if so.
                //
                // If you're here because a profiler sent you: I guess I should've
                // used a hashmap?
                let old_was_ime = old
                    .map(|old| {
                        self.ime_handlers
                            .iter()
                            .any(|(_, sesh)| sesh.widget_id == old)
                    })
                    .unwrap_or(false);
                let maybe_active_text_field = self
                    .ime_handlers
                    .iter()
                    .find(|(_, sesh)| Some(sesh.widget_id) == self.focus)
                    .map(|(token, _)| *token);
                // we call this on every focus change; we could call it less but does it matter?
                self.ime_focus_change = if maybe_active_text_field.is_some() {
                    Some(maybe_active_text_field)
                } else if old_was_ime {
                    Some(None)
                } else {
                    None
                };
            }
        }
    }

    /// Create a function that can invalidate the provided widget's text state.
    ///
    /// This will be called from outside the main app state in order to avoid
    /// reentrancy problems.
    pub(crate) fn ime_invalidation_fn(&self, widget: WidgetId) -> Option<Box<ImeUpdateFn>> {
        let token = self
            .ime_handlers
            .iter()
            .find(|(_, reg)| reg.widget_id == widget)
            .map(|(t, _)| *t)?;
        let window_handle = self.handle.clone();
        Some(Box::new(move |event| {
            window_handle.update_text_field(token, event)
        }))
    }

    /// Release a lock on an IME session, returning a `WidgetId` if the lock was mutable.
    ///
    /// After a mutable lock is released, the widget needs to be notified so that
    /// it can update any internal state.
    pub(crate) fn release_ime_lock(&mut self, req_token: TextFieldToken) -> Option<WidgetId> {
        self.ime_handlers
            .iter()
            .find(|(token, _)| req_token == *token)
            .and_then(|(_, reg)| reg.document.release().then(|| reg.widget_id))
    }

    fn widget_for_focus_request(&self, focus: FocusChange) -> Option<WidgetId> {
        match focus {
            FocusChange::Resign => None,
            FocusChange::Focus(id) => Some(id),
            FocusChange::Next => self.widget_from_focus_chain(true),
            FocusChange::Previous => self.widget_from_focus_chain(false),
        }
    }

    fn widget_from_focus_chain(&self, forward: bool) -> Option<WidgetId> {
        self.focus.and_then(|focus| {
            self.focus_chain()
                .iter()
                // Find where the focused widget is in the focus chain
                .position(|id| id == &focus)
                .map(|idx| {
                    // Return the id that's next to it in the focus chain
                    let len = self.focus_chain().len();
                    let new_idx = if forward {
                        (idx + 1) % len
                    } else {
                        (idx + len - 1) % len
                    };
                    self.focus_chain()[new_idx]
                })
                .or_else(|| {
                    // If the currently focused widget isn't in the focus chain,
                    // then we'll just return the first/last entry of the chain, if any.
                    if forward {
                        self.focus_chain().first().copied()
                    } else {
                        self.focus_chain().last().copied()
                    }
                })
        })
    }
}