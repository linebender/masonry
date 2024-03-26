// FIXME - Remove this
#![allow(unused)]

use std::cell::{RefCell, RefMut};
use std::collections::{HashMap, VecDeque};
use std::ops::DerefMut;
use std::rc::Rc;

use druid_shell::text::InputHandler;
// TODO - rename Application to AppHandle in glazier
// See https://github.com/linebender/glazier/issues/44
use druid_shell::{Application as AppHandle, WindowHandle};
use druid_shell::{
    Cursor, FileDialogToken, FileInfo, Region, TextFieldToken, TimerToken, WindowBuilder,
};
// Automatically defaults to std::time::Instant on non Wasm platforms
use instant::Instant;
use tracing::{error, info, info_span};
use vello::Scene;
use winit::{
    dpi::{PhysicalPosition, Size},
    window::CursorIcon,
};

use crate::action::ActionQueue;
use crate::app_delegate::{AppDelegate, DelegateCtx, NullDelegate};
use crate::command::CommandQueue;
use crate::contexts::GlobalPassCtx;
use crate::debug_logger::DebugLogger;
use crate::event2::{PointerEvent, TextEvent, WidgetEvent, WindowEvent};
use crate::ext_event::{ExtEventQueue, ExtEventSink, ExtMessage};
use crate::kurbo::{Point, Size};
use crate::piet::{Color, Piet, RenderContext};
use crate::platform::{
    DialogInfo, WindowConfig, WindowSizePolicy, EXT_EVENT_IDLE_TOKEN, RUN_COMMANDS_TOKEN,
};
use crate::testing::MockTimerQueue;
use crate::text::TextFieldRegistration;
use crate::widget::{FocusChange, StoreInWidgetMut, WidgetMut, WidgetRef, WidgetState};
use crate::{
    command as sys_cmd, Action, ArcStr, BoxConstraints, Command, Env, Event, EventCtx, Handled,
    InternalEvent, InternalLifeCycle, LayoutCtx, LifeCycle, LifeCycleCtx, MasonryWinHandler,
    PaintCtx, PlatformError, Target, Widget, WidgetCtx, WidgetId, WidgetPod, WindowDescription,
    WindowId,
};

pub struct RenderRoot {
    root: WidgetPod<Box<dyn Widget>>,
    size: Size,
    /// Is `Some` if the most recently displayed frame was an animation frame.
    last_anim: Option<Instant>,
    last_mouse_pos: Option<PhysicalPosition<f64>>,
    focused_widget: Option<WidgetId>,
    cursor_icon: CursorIcon,
    signal_queue: VecDeque<RenderRootSignal>,
}

// TODO - Migrate evrything in GlobalPassCtx into this struct
// Then have FoobarCtx types hold a reference to this struct; have RenderRoot own
// the only instance. This should fix lifetime issues.
#[cfg(FALSE)]
pub(crate) struct RenderRootState {
    // TODO
}

/*
TODO - Document things that didn't translate from druid:
    pub(crate) id: WindowId,
    pub(crate) title: ArcStr,
    size_policy: WindowSizePolicy,
    invalid: Region,
    pub(crate) ext_event_sink: ExtEventSink,
    pub(crate) timers: HashMap<TimerToken, WidgetId>,
    // Used in unit tests - see `src/testing/mock_timer_queue.rs`
    pub(crate) mock_timer_queue: Option<MockTimerQueue>,
    pub(crate) transparent: bool,
    pub(crate) ime_handlers: Vec<(TextFieldToken, TextFieldRegistration)>,
    pub(crate) ime_focus_change: Option<Option<TextFieldToken>>,
*/

pub struct WorkerCtx {
    // TODO
}

pub struct WorkerFn(pub Box<dyn FnOnce(WorkerCtx) + Send + 'static>);

// TODO - Handle custom cursors?
// TODO - handling timers
pub enum RenderRootSignal {
    Action(Action),
    // TODO?
    TextFieldAdded,
    TextFieldRemoved,
    ImeStarted,
    ImeMoved,
    RequestRedraw,
    RequestIdle,
    RequestAnimFrame,
    SpawnWorker(WorkerFn),
    TakeFocus,
    SetCursor(CursorIcon),
    // TODO - replace with PhysicalSize?
    SetSize(Size),
    SetTitle(String),
}

impl RenderRoot {
    pub fn new() -> Self {
        //
    }

    pub fn handle_window_event(&mut self, event: WindowEvent) -> Handled {
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
                &LifeCycle::Internal(InternalLifeCycle::RouteWidgetAdded),
                debug_logger,
                command_queue,
                action_queue,
                env,
                false,
            );
        }

        // TODO
        Handled::No
    }

    pub fn handle_pointer_event(&mut self, event: PointerEvent) -> Handled {
        //
    }

    pub fn handle_text_event(&mut self, event: TextEvent) -> Handled {
        //
    }

    pub fn redraw(&mut self) -> Scene {
        // TODO - call Xilem's reconciliation logic?

        // root_layout();
        // scene.clear();
        // root_paint();
        // TODO - handle case where layout/paint produces layout changes
    }

    pub fn pop_signal(&mut self) -> Option<RenderRootSignal> {
        //
    }

    fn root_on_event(
        &mut self,
        debug_logger: &mut DebugLogger,
        command_queue: &mut CommandQueue,
        action_queue: &mut ActionQueue,
        event: WidgetEvent,
        env: &Env,
    ) -> Handled {
        let mut widget_state = WidgetState::new(self.root.id(), Some(self.size), "<root>");
        let is_handled = {
            let mut global_state = GlobalPassCtx::new(
                self.ext_event_sink.clone(),
                debug_logger,
                command_queue,
                action_queue,
                &mut self.timers,
                self.mock_timer_queue.as_mut(),
                &self.handle,
                self.id,
                self.focus,
            );
            let mut notifications = VecDeque::new();

            let mut ctx = EventCtx {
                global_state: &mut global_state,
                widget_state: &mut widget_state,
                notifications: &mut notifications,
                is_handled: false,
                is_root: true,
                request_pan_to_child: None,
            };

            {
                ctx.global_state
                    .debug_logger
                    .push_important_span(&format!("EVENT {}", event.short_name()));
                let _span = info_span!("event").entered();
                self.root.on_event(&mut ctx, &event, env);
                ctx.global_state.debug_logger.pop_span();
            }

            if !ctx.notifications.is_empty() {
                info!("{} unhandled notifications:", ctx.notifications.len());
                for (i, n) in ctx.notifications.iter().enumerate() {
                    info!("{}: {:?}", i, n);
                }
            }

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
                self.layout(debug_logger, command_queue, action_queue, env);
            }

            self.post_event_processing(
                &mut widget_state,
                debug_logger,
                command_queue,
                action_queue,
                env,
                false,
            );

            self.root.as_dyn().debug_validate(false);

            Handled::from(ctx.is_handled)
        };
        Handled::No
    }

    fn root_lifecycle(
        &mut self,
        debug_logger: &mut DebugLogger,
        command_queue: &mut CommandQueue,
        action_queue: &mut ActionQueue,
        env: &Env,
        // TODO - Remove
        process_commands: bool,
    ) {
        let mut widget_state = WidgetState::new(self.root.id(), Some(self.size), "<root>");
        let mut global_state = GlobalPassCtx::new(
            self.ext_event_sink.clone(),
            debug_logger,
            command_queue,
            action_queue,
            &mut self.timers,
            self.mock_timer_queue.as_mut(),
            &self.handle,
            self.id,
            self.focus,
        );
        let mut ctx = LifeCycleCtx {
            global_state: &mut global_state,
            widget_state: &mut widget_state,
        };

        {
            ctx.global_state
                .debug_logger
                .push_important_span(&format!("LIFECYCLE {}", event.short_name()));
            let _span = info_span!("lifecycle").entered();
            self.root.lifecycle(&mut ctx, event, env);
            ctx.global_state.debug_logger.pop_span();
        }

        self.post_event_processing(
            &mut widget_state,
            debug_logger,
            command_queue,
            action_queue,
            env,
            process_commands,
        );
    }

    fn root_layout(
        &mut self,
        debug_logger: &mut DebugLogger,
        command_queue: &mut CommandQueue,
        action_queue: &mut ActionQueue,
        env: &Env,
    ) {
        let mut widget_state = WidgetState::new(self.root.id(), Some(self.size), "<root>");
        let mut global_state = GlobalPassCtx::new(
            self.ext_event_sink.clone(),
            debug_logger,
            command_queue,
            action_queue,
            &mut self.timers,
            self.mock_timer_queue.as_mut(),
            &self.handle,
            self.id,
            self.focus,
        );
        let mut layout_ctx = LayoutCtx {
            global_state: &mut global_state,
            widget_state: &mut widget_state,
            mouse_pos: self.last_mouse_pos,
        };
        let bc = match self.size_policy {
            WindowSizePolicy::User => BoxConstraints::tight(self.size),
            WindowSizePolicy::Content => BoxConstraints::UNBOUNDED,
        };

        let content_size = {
            layout_ctx
                .global_state
                .debug_logger
                .push_important_span("LAYOUT");
            let _span = info_span!("layout").entered();
            self.root.layout(&mut layout_ctx, &bc, env)
        };
        layout_ctx.global_state.debug_logger.pop_span();

        if let WindowSizePolicy::Content = self.size_policy {
            let insets = self.handle.content_insets();
            let full_size = (content_size.to_rect() + insets).size();
            if self.size != full_size {
                self.size = full_size;
                self.handle.set_size(full_size)
            }
        }
        layout_ctx.place_child(&mut self.root, Point::ORIGIN, env);
        self.lifecycle(
            &LifeCycle::Internal(InternalLifeCycle::ParentWindowOrigin),
            debug_logger,
            command_queue,
            action_queue,
            env,
            false,
        );
        self.post_event_processing(
            &mut widget_state,
            debug_logger,
            command_queue,
            action_queue,
            env,
            true,
        );
    }

    fn root_paint(
        &mut self,
        debug_logger: &mut DebugLogger,
        command_queue: &mut CommandQueue,
        action_queue: &mut ActionQueue,
        env: &Env,
    ) -> Scene {
        // TODO - Handle Xilem's VIEW_CONTEXT_CHANGED

        let widget_state = WidgetState::new(self.root.id(), Some(self.size), "<root>");
        let mut global_state = GlobalPassCtx::new(
            self.ext_event_sink.clone(),
            debug_logger,
            command_queue,
            action_queue,
            &mut self.timers,
            self.mock_timer_queue.as_mut(),
            &self.handle,
            self.id,
            self.focus,
        );
        let mut ctx = PaintCtx {
            render_ctx: piet,
            global_state: &mut global_state,
            widget_state: &widget_state,
            z_ops: Vec::new(),
            region: invalid.clone(),
            depth: 0,
        };

        let root_pod = self.root_pod.as_mut().unwrap();
        let mut cx_state =
            CxState::new(&mut self.font_cx, &self.cx.tree_structure, &mut self.events);
        let mut paint_cx = PaintCx::new(&mut cx_state, &mut self.root_state);
        root_pod.paint_impl(&mut paint_cx);

        // FIXME
        Scene::new()
    }

    fn post_event_processing(
        &mut self,
        widget_state: &mut WidgetState,
        debug_logger: &mut DebugLogger,
        command_queue: &mut CommandQueue,
        action_queue: &mut ActionQueue,
        env: &Env,
        process_commands: bool,
    ) {
        // TODO - Remove calls to lifecycle from this function
        // TODO - process_commands

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
                &LifeCycle::Internal(InternalLifeCycle::RouteWidgetAdded),
                debug_logger,
                command_queue,
                action_queue,
                env,
                false,
            );
        }

        if debug_logger.layout_tree.root.is_none() {
            debug_logger.layout_tree.root = Some(self.root.id().to_raw() as u32);
        }

        if self.root.state().needs_window_origin && !self.root.state().needs_layout {
            let event = LifeCycle::Internal(InternalLifeCycle::ParentWindowOrigin);
            self.lifecycle(
                &event,
                debug_logger,
                command_queue,
                action_queue,
                env,
                false,
            );
        }

        // Update the disabled state if necessary
        // Always do this before updating the focus-chain
        if self.root.state().tree_disabled_changed() {
            let event = LifeCycle::Internal(InternalLifeCycle::RouteDisabledChanged);
            self.lifecycle(
                &event,
                debug_logger,
                command_queue,
                action_queue,
                env,
                false,
            );
        }

        // Update the focus-chain if necessary
        // Always do this before sending focus change, since this event updates the focus chain.
        if self.root.state().update_focus_chain {
            let event = LifeCycle::BuildFocusChain;
            self.lifecycle(
                &event,
                debug_logger,
                command_queue,
                action_queue,
                env,
                false,
            );
        }

        self.update_focus(widget_state, debug_logger, command_queue, action_queue, env);

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
        if process_commands && !command_queue.is_empty() {
            // Ask the handler to call us back on idle
            // so we can process them in a new event/update pass.
            if let Some(mut handle) = self.handle.get_idle_handle() {
                handle.schedule_idle(RUN_COMMANDS_TOKEN);
            } else {
                // FIXME - probably messes with tests
                error!("failed to get idle handle");
            }
        }
    }

    fn update_focus(
        &mut self,
        widget_state: &mut WidgetState,
        debug_logger: &mut DebugLogger,
        command_queue: &mut CommandQueue,
        action_queue: &mut ActionQueue,
        env: &Env,
    ) {
        if let Some(focus_req) = widget_state.request_focus.take() {
            let old = self.focus;
            let new = self.widget_for_focus_request(focus_req);

            // TODO
            // Skip change if requested widget is disabled

            // Only send RouteFocusChanged in case there's actual change
            if old != new {
                let event = LifeCycle::Internal(InternalLifeCycle::RouteFocusChanged { old, new });
                self.lifecycle(
                    &event,
                    debug_logger,
                    command_queue,
                    action_queue,
                    env,
                    false,
                );
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
}

/*
TODO:
- Invalidation regions
- Timer handling
- prepare_paint
- Focus-related stuff
*/
