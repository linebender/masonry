use vello::Scene;
use winit::{dpi::Size, window::CursorIcon};

use crate::{
    event2::{PointerEvent, TextEvent, WindowEvent},
    Action, Handled,
};

pub struct RenderRoot {
    //
}

// TODO - Handle custom cursors?
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
    // TODO - handling timers
    RequestTimer(u32),
    SpawnWorker(TODO),
    TakeFocus,
    SetCursor(CursorIcon),
    // TODO - replace with PhysicalSize?
    SetSize(Size),
}

// impl FnOnce(WorkerCtx) + Send + 'static

impl RenderRoot {
    pub fn new() -> Self {
        //
    }

    pub fn handle_window_event(&mut self, event: WindowEvent) -> Handled {
        //
    }

    pub fn handle_pointer_event(&mut self, event: PointerEvent) -> Handled {
        //
    }

    pub fn handle_text_event(&mut self, event: TextEvent) -> Handled {
        //
    }

    pub fn paint(&mut self) -> Scene {
        //
    }

    pub fn pop_signal(&mut self) -> Option<RenderRootSignal> {
        //
    }
}
