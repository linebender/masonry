use super::reactive::{create_effect, update_widget};
use super::view::{AnyView, View, ViewSeq};
use crate::mini::reactive::untrack;
use crate::{
    widget::{Axis, Flex, Label, Portal, SizedBox, Stack},
    Widget, WidgetId,
};
use piet_common::{kurbo::Point, Color};
use std::{any::Any, fmt::Display};

pub fn text<S: Display>(text: S) -> View<impl Widget> {
    View::new(
        Label::new(text.to_string())
            .with_text_color(Color::BLACK)
            .with_text_size(12.0),
    )
}

pub fn container(child: impl Widget) -> View<impl Widget> {
    View::new(SizedBox::new(child))
}

pub fn empty() -> View<impl Widget> {
    View::new(SizedBox::empty())
}

fn stack_from_iter<W: 'static>(
    axis: Axis,
    iterator: impl IntoIterator<Item = View<W>>,
) -> View<impl Widget> {
    let mut flex = Flex::for_axis(axis);
    let iter = iterator.into_iter();
    for child in iter {
        if child.grow {
            flex = flex.with_flex_child(child, 1.0)
        } else {
            flex = flex.with_child(child);
        }
    }
    View::new(flex)
}

pub fn h_stack(children: impl ViewSeq) -> View<impl Widget> {
    stack_from_iter(Axis::Horizontal, children.views())
}

pub fn v_stack(children: impl ViewSeq) -> View<impl Widget> {
    stack_from_iter(Axis::Vertical, children.views())
}

/// Creates a stack from an iterator of widgets.
#[allow(unused)]
pub fn h_stack_from_iter<W: 'static>(
    iterator: impl IntoIterator<Item = View<W>>,
) -> View<impl Widget> {
    stack_from_iter(Axis::Horizontal, iterator)
}

/// Creates a stack from an iterator of widgets.
pub fn v_stack_from_iter<W: 'static>(
    iterator: impl IntoIterator<Item = View<W>>,
) -> View<impl Widget> {
    stack_from_iter(Axis::Vertical, iterator)
}

pub fn z_stack_from_iter<W: 'static>(
    iterator: impl IntoIterator<Item = (View<W>, WidgetId, Point)>,
) -> View<impl Widget> {
    let mut stack = Stack::new();
    let iter = iterator.into_iter();
    for (child, id, pos) in iter {
        stack = stack.with_child_id(child, pos, id)
    }
    View::new(stack)
}

pub fn scroll(child: View<impl Any>) -> View<impl Widget> {
    let child: Box<dyn Widget> = Box::new(child);
    View::new(Portal::new(child))
}

pub fn button<S: Display + 'static>(label: impl Fn() -> S + 'static) -> View<impl Widget> {
    container(text(label()).style(|s| {
        s.background(Color::rgb8(240, 240, 240))
            .padding(5.0)
            .color(Color::rgb8(40, 40, 40))
            .border(1.0)
            .border_color(Color::rgb8(140, 140, 140))
            .border_radius(5.0)
    }))
}

pub fn dyn_container<T: Send + 'static>(
    update_view: impl Fn() -> T + 'static,
    child_fn: impl Fn(T) -> AnyView + 'static,
) -> View<impl Widget> {
    let view = empty();
    let id = view.id();
    create_effect(move || {
        let value = update_view();
        untrack(|| {
            let widget = child_fn(value);
            update_widget::<SizedBox>(id, |mut sized| sized.set_child(widget));
        });
    });

    view
}
