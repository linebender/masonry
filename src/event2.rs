use std::{collections::HashSet, path::PathBuf};

use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceId, Ime, KeyEvent, Modifiers, MouseButton, MouseScrollDelta},
    keyboard::ModifiersState,
};

// TODO - Occluded(bool) event
// TODO - winit ActivationTokenDone thing
// TODO - Suspended/Resume/NewEvents/MemoryWarning
// TODO - wtf is InnerSizeWriter?
#[derive(Debug, Clone)]
pub enum WindowEvent {
    RequestClose,
    // TODO - just add close() method instead?
    Destroyed,
    Rescale(f64),
    Resize(PhysicalSize<u32>),
    Move(PhysicalPosition<i32>),
    ChangeTheme(WindowTheme),
    AnimFrame,
}

// TODO - Move AnimFrame to Lifecycle

// TODO - How can RenderRoot express "I started a drag-and-drop op"?
// TODO - Touchpad, Touch, AxisMotion
// TODO - How to handle CursorEntered?
// Note to self: Events like "pointerenter", "pointerleave" are handled differently at the Widget level. But that's weird because WidgetPod can distribute them. Need to think about this again.
pub enum PointerEvent {
    PointerDown(MouseButton, PointerState),
    PointerUp(MouseButton, PointerState),
    PointerMove(PointerState),
    PointerEnter(PointerState),
    PointerLeave(PointerState),
    MouseWheel(MouseScrollDelta, PointerState),
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

// FIXME - Remove this
#[derive(Debug, Clone)]
pub enum WidgetEvent {
    PointerEvent(PointerEvent),
    TextEvent(TextEvent),
    WindowEvent(WindowEvent),
}

pub enum WindowTheme {
    Light,
    Dark,
}
