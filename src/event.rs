// This software is licensed under Apache License 2.0 and distributed on an
// "as-is" basis without warranties of any kind. See the LICENSE file for
// details.

//! Events.

use crate::kurbo::Rect;
// TODO - See issue #14
use crate::WidgetId;

use std::{collections::HashSet, path::PathBuf};

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{DeviceId, Ime, KeyEvent, Modifiers, MouseButton};
use winit::keyboard::ModifiersState;

// TODO - Occluded(bool) event
// TODO - winit ActivationTokenDone thing
// TODO - Suspended/Resume/NewEvents/MemoryWarning
// TODO - wtf is InnerSizeWriter?
// TODO - Move AnimFrame to Lifecycle
// TODO - switch anim frames to being about age / an absolute timestamp
// instead of time elapsed.
// (this will help in cases where we want to skip anim frames)
#[derive(Debug, Clone)]
pub enum WindowEvent {
    Rescale(f64),
    Resize(PhysicalSize<u32>),
    AnimFrame,
}

// TODO - How can RenderRoot express "I started a drag-and-drop op"?
// TODO - Touchpad, Touch, AxisMotion
// TODO - How to handle CursorEntered?
// Note to self: Events like "pointerenter", "pointerleave" are handled differently at the Widget level. But that's weird because WidgetPod can distribute them. Need to think about this again.
#[derive(Debug, Clone)]
pub enum PointerEvent {
    PointerDown(MouseButton, PointerState),
    PointerUp(MouseButton, PointerState),
    PointerMove(PointerState),
    PointerEnter(PointerState),
    PointerLeave(PointerState),
    MouseWheel(PhysicalPosition<f64>, PointerState),
    HoverFile(PathBuf, PointerState),
    DropFile(PathBuf, PointerState),
    HoverFileCancel(PointerState),
}

// TODO - Clipboard Paste?
// TODO skip is_synthetic=true events
#[derive(Debug, Clone)]
pub enum TextEvent {
    KeyboardKey(KeyEvent, ModifiersState),
    Ime(Ime),
    ModifierChange(ModifiersState),
    // TODO - Document difference with Lifecycle focus change
    FocusChange(bool),
}

#[derive(Debug, Clone)]
pub struct PointerState {
    pub device_id: DeviceId,
    pub position: PhysicalPosition<f64>,
    pub buttons: HashSet<MouseButton>,
    pub mods: Modifiers,
    pub count: u8,
    pub focus: bool,
}

#[derive(Debug, Clone)]
pub enum WindowTheme {
    Light,
    Dark,
}

/// Application life cycle events.
///
/// Unlike [`Event`]s, [`LifeCycle`] events are generated by Masonry, and
/// may occur at different times during a given pass of the event loop. The
/// [`LifeCycle::WidgetAdded`] event, for instance, may occur when the app
/// first launches (during the handling of [`Event::WindowConnected`]) or it
/// may occur during an [`on_event`](crate::Widget::on_event) pass, if some
/// widget has been added then.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum LifeCycle {
    /// Sent to a `Widget` when it is added to the widget tree. This should be
    /// the first message that each widget receives.
    ///
    /// Widgets should handle this event in order to do any initial setup.
    ///
    /// In addition to setup, this event is also used by the framework to
    /// track certain types of important widget state.
    ///
    /// ## Registering children
    ///
    /// Container widgets (widgets which use [`WidgetPod`](crate::WidgetPod) to
    /// manage children) must ensure that this event is forwarded to those children.
    /// The [`WidgetPod`](crate::WidgetPod) itself will handle registering those
    /// children with the system; this is required for things like correct routing
    /// of events.
    WidgetAdded,

    /// Called at the beginning of a new animation frame.
    ///
    /// On the first frame when transitioning from idle to animating, `interval`
    /// will be 0. (This logic is presently per-window but might change to
    /// per-widget to make it more consistent). Otherwise it is in nanoseconds.
    ///
    /// The `paint` method will be called shortly after this event is finished.
    /// As a result, you should try to avoid doing anything computationally
    /// intensive in response to an `AnimFrame` event: it might make the app miss
    /// the monitor's refresh, causing lag or jerky animations.
    AnimFrame(u64),

    // TODO - Put in StatusChange
    /// Called when the Disabled state of the widgets is changed.
    ///
    /// To check if a widget is disabled, see [`is_disabled`].
    ///
    /// To change a widget's disabled state, see [`set_disabled`].
    ///
    /// [`is_disabled`]: crate::EventCtx::is_disabled
    /// [`set_disabled`]: crate::EventCtx::set_disabled
    DisabledChanged(bool),

    /// Called when the widget tree changes and Masonry wants to rebuild the
    /// Focus-chain.
    ///
    /// It is the only place from which [`register_for_focus`] should be called.
    /// By doing so the widget can get focused by other widgets using [`focus_next`] or [`focus_prev`].
    ///
    /// [`register_for_focus`]: crate::LifeCycleCtx::register_for_focus
    /// [`focus_next`]: crate::EventCtx::focus_next
    /// [`focus_prev`]: crate::EventCtx::focus_prev
    BuildFocusChain,

    /// Called when a child widgets uses
    /// [`EventCtx::request_pan_to_this`](crate::EventCtx::request_pan_to_this).
    RequestPanToChild(Rect),

    /// Internal Masonry lifecycle event.
    ///
    /// This should always be passed down to descendant [`WidgetPod`]s.
    ///
    /// [`WidgetPod`]: struct.WidgetPod.html
    Internal(InternalLifeCycle),
}

/// Internal lifecycle events used by Masonry inside [`WidgetPod`].
///
/// These events are translated into regular [`LifeCycle`] events
/// and should not be used directly.
///
/// [`WidgetPod`]: struct.WidgetPod.html
/// [`LifeCycle`]: enum.LifeCycle.html
#[derive(Debug, Clone)]
pub enum InternalLifeCycle {
    /// Used to route the `WidgetAdded` event to the required widgets.
    RouteWidgetAdded,

    /// Used to route the `FocusChanged` event.
    RouteFocusChanged {
        /// the widget that is losing focus, if any
        old: Option<WidgetId>,
        /// the widget that is gaining focus, if any
        new: Option<WidgetId>,
    },

    /// Used to route the `DisabledChanged` event to the required widgets.
    RouteDisabledChanged,

    /// The parents widget origin in window coordinate space has changed.
    ParentWindowOrigin,
}

/// Event indicating status changes within the widget hierarchy.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum StatusChange {
    /// Called when the "hot" status changes.
    ///
    /// This will always be called _before_ the event that triggered it; that is,
    /// when the mouse moves over a widget, that widget will receive
    /// `StatusChange::HotChanged` before it receives `Event::MouseMove`.
    ///
    /// See [`is_hot`](struct.EventCtx.html#method.is_hot) for
    /// discussion about the hot status.
    HotChanged(bool),

    /// Called when the focus status changes.
    ///
    /// This will always be called immediately after a new widget gains focus.
    /// The newly focused widget will receive this with `true` and the widget
    /// that lost focus will receive this with `false`.
    ///
    /// See [`EventCtx::is_focused`] for more information about focus.
    ///
    /// [`EventCtx::is_focused`]: struct.EventCtx.html#method.is_focused
    FocusChanged(bool),
}

impl PointerEvent {
    pub fn pointer_state(&self) -> &PointerState {
        match self {
            PointerEvent::PointerDown(_, state)
            | PointerEvent::PointerUp(_, state)
            | PointerEvent::PointerMove(state)
            | PointerEvent::PointerEnter(state)
            | PointerEvent::PointerLeave(state)
            | PointerEvent::MouseWheel(_, state)
            | PointerEvent::HoverFile(_, state)
            | PointerEvent::DropFile(_, state)
            | PointerEvent::HoverFileCancel(state) => state,
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            PointerEvent::PointerDown(_, _) => "PointerDown",
            PointerEvent::PointerUp(_, _) => "PointerUp",
            PointerEvent::PointerMove(_) => "PointerMove",
            PointerEvent::PointerEnter(_) => "PointerEnter",
            PointerEvent::PointerLeave(_) => "PointerLeave",
            PointerEvent::MouseWheel(_, _) => "MouseWheel",
            PointerEvent::HoverFile(_, _) => "HoverFile",
            PointerEvent::DropFile(_, _) => "DropFile",
            PointerEvent::HoverFileCancel(_) => "HoverFileCancel",
        }
    }
}

impl TextEvent {
    pub fn short_name(&self) -> &'static str {
        match self {
            TextEvent::KeyboardKey(_, _) => "KeyboardKey",
            TextEvent::Ime(_) => "Ime",
            TextEvent::ModifierChange(_) => "ModifierChange",
            TextEvent::FocusChange(_) => "FocusChange",
        }
    }
}

impl PointerState {
    pub fn empty() -> Self {
        #[allow(unsafe_code)]
        PointerState {
            // SAFETY: Uuuuh, unclear. Winit says the dummy id should only be used in
            // tests and should never be passed to winit. In principle, we're never
            // passing this id to winit, but it's still visible to custom widgets which
            // might do so if they tried really hard.
            // It would be a lot better if winit could just make this constructor safe.
            device_id: unsafe { DeviceId::dummy() },
            position: PhysicalPosition::new(0.0, 0.0),
            buttons: Default::default(),
            mods: Default::default(),
            count: 0,
            focus: false,
        }
    }
}

impl LifeCycle {
    // TODO - link this to documentation of stashed widgets - See issue #9
    /// Whether this event should be sent to widgets which are currently not visible and not
    /// accessible.
    ///
    /// If a widget changes which children are `hidden` it must call [`children_changed`].
    /// For a more detailed explanation of the `hidden` state, see [`Event::should_propagate_to_hidden`].
    ///
    /// [`children_changed`]: crate::EventCtx::children_changed
    /// [`Event::should_propagate_to_hidden`]: Event::should_propagate_to_hidden
    pub fn should_propagate_to_hidden(&self) -> bool {
        match self {
            LifeCycle::Internal(internal) => internal.should_propagate_to_hidden(),
            LifeCycle::WidgetAdded => true,
            LifeCycle::AnimFrame(_) => true,
            LifeCycle::DisabledChanged(_) => true,
            LifeCycle::BuildFocusChain => false,
            LifeCycle::RequestPanToChild(_) => false,
        }
    }

    /// Short name, for debug logging.
    ///
    /// Essentially returns the enum variant name.
    pub fn short_name(&self) -> &str {
        match self {
            LifeCycle::Internal(internal) => match internal {
                InternalLifeCycle::RouteWidgetAdded => "RouteWidgetAdded",
                InternalLifeCycle::RouteFocusChanged { .. } => "RouteFocusChanged",
                InternalLifeCycle::RouteDisabledChanged => "RouteDisabledChanged",
                InternalLifeCycle::ParentWindowOrigin => "ParentWindowOrigin",
            },
            LifeCycle::WidgetAdded => "WidgetAdded",
            LifeCycle::AnimFrame(_) => "AnimFrame",
            LifeCycle::DisabledChanged(_) => "DisabledChanged",
            LifeCycle::BuildFocusChain => "BuildFocusChain",
            LifeCycle::RequestPanToChild(_) => "RequestPanToChild",
        }
    }
}

impl InternalLifeCycle {
    /// Whether this event should be sent to widgets which are currently not visible and not
    /// accessible.
    ///
    /// If a widget changes which children are `hidden` it must call [`children_changed`].
    /// For a more detailed explanation of the `hidden` state, see [`Event::should_propagate_to_hidden`].
    ///
    /// [`children_changed`]: crate::EventCtx::children_changed
    /// [`Event::should_propagate_to_hidden`]: Event::should_propagate_to_hidden
    pub fn should_propagate_to_hidden(&self) -> bool {
        match self {
            InternalLifeCycle::RouteWidgetAdded
            | InternalLifeCycle::RouteFocusChanged { .. }
            | InternalLifeCycle::RouteDisabledChanged => true,
            InternalLifeCycle::ParentWindowOrigin => false,
        }
    }
}
