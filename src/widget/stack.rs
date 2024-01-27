// This software is licensed under Apache License 2.0 and distributed on an
// "as-is" basis without warranties of any kind. See the LICENSE file for
// details.

use crate::widget::WidgetRef;
use crate::{
    BoxConstraints, Env, Event, EventCtx, LayoutCtx, LifeCycle, LifeCycleCtx, PaintCtx, Point,
    Size, StatusChange, Widget, WidgetId, WidgetPod,
};
use smallvec::SmallVec;
use tracing::{trace_span, Span};

/// A container that stacks children at absolute positions.
pub struct Stack {
    children: Vec<Child>,
}

struct Child {
    widget: WidgetPod<Box<dyn Widget>>,
    position: Point,
}

crate::declare_widget!(StackMut, Stack);

impl Stack {
    pub fn new() -> Self {
        Stack {
            children: Vec::new(),
        }
    }

    /// Builder-style method to add a child to the container.
    pub fn with_child_id(
        mut self,
        child: impl Widget,
        position: impl Into<Point>,
        id: WidgetId,
    ) -> Self {
        self.children.push(Child {
            widget: WidgetPod::new_with_id(Box::new(child), id),
            position: position.into(),
        });
        self
    }
}

// --- Mutate live Stack - WidgetMut ---

impl<'a, 'b> StackMut<'a, 'b> {
    pub fn set_child_position(&mut self, child_id: WidgetId, position: Point) {
        if let Some(child) = self
            .widget
            .children
            .iter_mut()
            .find(|child| child.widget.id() == child_id)
        {
            child.position = position;
        }
        self.ctx.widget_state.needs_layout = true;
    }
}

impl Widget for Stack {
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) {
        for child in &mut self.children {
            child.widget.on_event(ctx, event, env);
        }
    }

    fn on_status_change(&mut self, _ctx: &mut LifeCycleCtx, _event: &StatusChange, _env: &Env) {}

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env) {
        for child in &mut self.children {
            child.widget.lifecycle(ctx, event, env);
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size {
        let mut result = Size::ZERO.to_rect();
        for child in &mut self.children {
            let size = child.widget.layout(ctx, bc, env).to_vec2() + child.position.to_vec2();
            ctx.place_child(&mut child.widget, child.position, env);
            result = result.union(size.to_size().to_rect());
        }
        bc.constrain(result.size())
    }

    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        for child in &mut self.children {
            child.widget.paint(ctx, env);
        }
    }

    fn children(&self) -> SmallVec<[WidgetRef<'_, dyn Widget>; 16]> {
        self.children
            .iter()
            .map(|child| child.widget.as_dyn())
            .collect()
    }

    fn make_trace_span(&self) -> Span {
        trace_span!("Stack")
    }
}
