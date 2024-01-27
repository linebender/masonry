use crate::mini::reactive::create_effect;
use crate::mini::reactive::{update_widget, update_widget_state};
use crate::{
    widget::{Portal, SizedBox, WidgetRef},
    Env, Event, EventCtx, LayoutCtx, LifeCycle, LifeCycleCtx, PaintCtx, StatusChange, Widget,
    WidgetId,
};
use crate::{BoxConstraints, WidgetPod};
use piet_common::kurbo::{Point, Size};
use smallvec::smallvec;
use smallvec::SmallVec;
use std::marker::PhantomData;
use tracing::{trace_span, Span};

type Id = WidgetId;

pub type AnyView = View<Box<dyn Widget>>;

pub struct View<W: 'static> {
    pub(super) grow: bool,
    widget: WidgetPod<SizedBox>,
    phantom: PhantomData<W>,
    on_any_event: Option<Box<dyn Fn(&Event)>>,
    on_enter: Option<Box<dyn Fn()>>,
    on_leave: Option<Box<dyn Fn()>>,
    on_click: Option<Box<dyn Fn(&Event)>>,
}

impl<W> View<W> {
    pub fn new(widget: W) -> Self
    where
        W: Widget,
    {
        View {
            grow: false,
            widget: WidgetPod::new(SizedBox::new(widget)),
            phantom: PhantomData,
            on_any_event: None,
            on_enter: None,
            on_leave: None,
            on_click: None,
        }
    }

    pub fn id(&self) -> WidgetId {
        self.widget.state.id
    }

    pub fn widget_id(&self) -> WidgetId {
        self.widget.inner.child.as_ref().unwrap().id()
    }

    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    pub fn on_any_event(mut self, action: impl Fn(&Event) + 'static) -> Self {
        self.on_any_event = Some(Box::new(action));
        self
    }

    pub fn on_enter(mut self, action: impl Fn() + 'static) -> Self {
        self.on_enter = Some(Box::new(action));
        self
    }

    pub fn on_leave(mut self, action: impl Fn() + 'static) -> Self {
        self.on_leave = Some(Box::new(action));
        self
    }

    pub fn on_click(mut self, action: impl Fn(&Event) + 'static) -> Self {
        self.on_click = Some(Box::new(action));
        self
    }

    pub fn scroll_to_view(self, view: impl Fn() -> Option<Id> + 'static) -> Self {
        // FIXME: This scrolling should be done after layout as there may not be a layout yet or
        // there's pending commands that could change layout.
        let id = self.widget_id();
        create_effect(move || {
            if let Some(view) = view() {
                update_widget_state(view, move |state| {
                    let layout = state.window_layout_rect();

                    update_widget::<Portal<Box<dyn Widget>>>(id, move |mut this| {
                        let origin = this.state().window_origin();
                        let baseline =
                            layout + this.last_layout_viewport_pos.to_vec2() - origin.to_vec2();
                        this.pan_viewport_to(baseline);
                    });
                });
            }
        });
        self
    }

    pub fn any(self) -> View<Box<dyn Widget>> {
        View {
            grow: self.grow,
            widget: self.widget,
            phantom: PhantomData,
            on_any_event: self.on_any_event,
            on_enter: self.on_enter,
            on_leave: self.on_leave,
            on_click: self.on_click,
        }
    }
}

impl<W> Widget for View<W> {
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) {
        if let Some(action) = self.on_any_event.as_ref() {
            action(event);
        }
        if let Event::MouseDown(_) = event {
            if let Some(action) = self.on_click.as_ref() {
                action(event);
            }
        }
        self.widget.on_event(ctx, event, env);
    }

    fn on_status_change(&mut self, _ctx: &mut LifeCycleCtx, event: &StatusChange, _env: &Env) {
        match event {
            StatusChange::HotChanged(status) => {
                if *status {
                    if let Some(action) = self.on_enter.as_ref() {
                        action();
                    }
                } else if let Some(action) = self.on_leave.as_ref() {
                    action();
                }
            }
            _ => (),
        }
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env) {
        self.widget.lifecycle(ctx, event, env);
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size {
        let size = self.widget.layout(ctx, bc, env);
        ctx.place_child(&mut self.widget, Point::ZERO, env);
        size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        self.widget.paint(ctx, env);
    }

    fn children(&self) -> SmallVec<[WidgetRef<'_, dyn Widget>; 16]> {
        smallvec![self.widget.as_dyn()]
    }

    fn make_trace_span(&self) -> Span {
        trace_span!("View")
    }
}

pub trait ViewSeq {
    fn views(self) -> Vec<AnyView>;
}

macro_rules! impl_widget_sec_for_tuple {
    ($n: tt; $($t:ident),*; $($i:tt),*; $($j:tt),*) => {
        impl< $( $t: 'static, )* > ViewSeq for ( $( View<$t>, )* ) {
            fn views(self) -> Vec<AnyView> {
                vec![$(self.$i.any(),)*]
            }
        }
    }
}

impl_widget_sec_for_tuple!(1; V0; 0; 0);
impl_widget_sec_for_tuple!(2; V0, V1; 0, 1; 1, 0);
impl_widget_sec_for_tuple!(3; V0, V1, V2; 0, 1, 2; 2, 1, 0);
impl_widget_sec_for_tuple!(4; V0, V1, V2, V3; 0, 1, 2, 3; 3, 2, 1, 0);
impl_widget_sec_for_tuple!(5; V0, V1, V2, V3, V4; 0, 1, 2, 3, 4; 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(6; V0, V1, V2, V3, V4, V5; 0, 1, 2, 3, 4, 5; 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(7; V0, V1, V2, V3, V4, V5, V6; 0, 1, 2, 3, 4, 5, 6; 6, 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(8; V0, V1, V2, V3, V4, V5, V6, V7; 0, 1, 2, 3, 4, 5, 6, 7; 7, 6, 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(9; V0, V1, V2, V3, V4, V5, V6, V7, V8; 0, 1, 2, 3, 4, 5, 6, 7, 8; 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(10; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9; 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(11; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10; 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(12; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11; 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(13; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12; 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(14; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13; 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(15; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14; 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
impl_widget_sec_for_tuple!(16; V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15; 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
