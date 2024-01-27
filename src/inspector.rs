use crate::command::{CommandQueue, INSPECT};
use crate::mini::reactive::{
    create_effect, create_rw_signal, update_widget, RuntimeView, RwSignal, CURRENT_RUNTIME,
};
use crate::mini::style::{LIGHT_GRAY, WHITE_SMOKE};
use crate::mini::view::{AnyView, View};
use crate::mini::views::{
    button, container, dyn_container, empty, h_stack, scroll, text, v_stack, v_stack_from_iter,
    z_stack_from_iter,
};
use crate::widget::{SizedBox, Stack, WidgetRef};
use crate::{
    BackgroundBrush, BoxConstraints, Command, Env, Event, EventCtx, LayoutCtx, LifeCycle,
    LifeCycleCtx, PaintCtx, StatusChange, Target, WidgetId, WindowId,
};
use crate::{Widget, WidgetPod};
use piet_common::kurbo::{Point, Rect, Size};
use piet_common::Color;
use smallvec::SmallVec;
use std::any::Any;
use std::cell::RefCell;
use std::fmt::Display;
use std::{rc::Rc, time::Instant};

type Id = WidgetId;

#[derive(Clone, Debug)]
pub struct CapturedView {
    id: Id,
    name: String,
    layout: Rect,
    clipped: Rect,
    children: Vec<Rc<CapturedView>>,
    keyboard_navigable: bool,
    focused: bool,
}

impl CapturedView {
    pub fn capture(widget: WidgetRef<'_, dyn Widget>, clip: Rect) -> Self {
        let id = widget.state().id;
        let layout = widget.state().window_layout_rect();
        let keyboard_navigable = false;
        let focused = false;
        let clipped = layout.intersect(clip);
        Self {
            id,
            name: widget.short_type_name().to_string(),
            layout,
            clipped,
            keyboard_navigable,
            focused,
            children: widget
                .children()
                .into_iter()
                .map(|view| Rc::new(CapturedView::capture(view, clipped)))
                .collect(),
        }
    }

    fn find(&self, id: Id) -> Option<&CapturedView> {
        if self.id == id {
            return Some(self);
        }
        self.children
            .iter()
            .filter_map(|child| child.find(id))
            .next()
    }

    fn find_by_pos(&self, pos: Point) -> Option<&CapturedView> {
        self.children
            .iter()
            .rev()
            .filter_map(|child| child.find_by_pos(pos))
            .next()
            .or_else(|| self.clipped.contains(pos).then_some(self))
    }

    fn warnings(&self) -> bool {
        self.children.iter().any(|child| child.warnings())
    }
}

struct Capture {
    root: Rc<CapturedView>,
    start: Instant,
    post_layout: Instant,
    end: Instant,
    window_size: Size,
    window_id: WindowId,
    background: Color,
}

fn captured_view_name(view: &CapturedView) -> View<impl Widget> {
    let name = text(view.name.clone());
    let id = text(view.id.to_raw()).style(|s| {
        s.margin_right(5.0)
            .background(Color::BLACK.with_alpha(0.02))
            .border(1.0)
            .border_radius(5.0)
            .border_color(Color::BLACK.with_alpha(0.07))
            .padding(3.0)
            .padding_top(0.0)
            .padding_bottom(0.0)
            .font_size(12.0)
            .color(Color::BLACK.with_alpha(0.6))
    });
    let tab = if view.focused {
        text("Focus")
            .style(|s| {
                s.margin_right(5.0)
                    .background(Color::rgb8(63, 81, 101).with_alpha(0.6))
                    .border_radius(5.0)
                    .padding(1.0)
                    .font_size(10.0)
                    .color(Color::WHITE.with_alpha(0.8))
            })
            .any()
    } else if view.keyboard_navigable {
        text("Tab")
            .style(|s| {
                s.margin_right(5.0)
                    .background(Color::rgb8(204, 217, 221).with_alpha(0.4))
                    .border(1.0)
                    .border_radius(5.0)
                    .border_color(Color::BLACK.with_alpha(0.07))
                    .padding(1.0)
                    .font_size(10.0)
                    .color(Color::BLACK.with_alpha(0.4))
            })
            .any()
    } else {
        empty().any()
    };
    h_stack((id, tab, name)).style(|s| s.items_center())
}

// Outlined to reduce stack usage.
#[inline(never)]
fn captured_view_no_children(
    view: &CapturedView,
    depth: usize,
    capture_view: &CaptureView,
) -> AnyView {
    let offset = depth as f64 * 14.0;
    let name = captured_view_name(view);
    let name_id = name.id();
    let height = 20.0;
    let id = view.id;
    let selected = capture_view.selected;
    let highlighted = capture_view.highlighted;

    let row = h_stack((empty().style(move |s| s.width(12.0 + offset)), name))
        .style(move |s| {
            s.hover(move |s| {
                s.background(Color::rgba8(228, 237, 216, 160))
                    .apply_if(selected.get() == Some(id), |s| {
                        s.background(Color::rgb8(186, 180, 216))
                    })
            })
            .height(height)
            .apply_if(highlighted.get() == Some(id), |s| {
                s.background(Color::rgba8(228, 237, 216, 160))
            })
            .apply_if(selected.get() == Some(id), |s| {
                if highlighted.get() == Some(id) {
                    s.background(Color::rgb8(186, 180, 216))
                } else {
                    s.background(Color::rgb8(213, 208, 216))
                }
            })
        })
        .on_click(move |_| selected.set(Some(id)))
        .on_enter(move || highlighted.set(Some(id)))
        .any();

    let row_id = row.id();
    let scroll_to = capture_view.scroll_to;
    let expanding_selection = capture_view.expanding_selection;
    create_effect(move || {
        if let Some(selection) = expanding_selection.get() {
            if selection == id {
                // Scroll to the row, then to the name part of the row.
                scroll_to.set(Some(row_id));
                scroll_to.set(Some(name_id));
            }
        }
    });

    row
}

// Outlined to reduce stack usage.
#[inline(never)]
fn captured_view_with_children(
    view: &Rc<CapturedView>,
    depth: usize,
    capture_view: &CaptureView,
    children: Vec<AnyView>,
) -> AnyView {
    let offset = depth as f64 * 14.0;
    let name = captured_view_name(view);
    let height = 20.0;
    let id = view.id;
    let selected = capture_view.selected;
    let highlighted = capture_view.highlighted;
    let expanding_selection = capture_view.expanding_selection;
    let view_ = view.clone();

    let expanded = create_rw_signal(true);

    let name_id = name.id();
    let row = h_stack((
        empty().style(move |s| s.width(offset)),
        empty()
            .style(move |s| {
                s.background(if expanded.get() {
                    Color::WHITE.with_alpha(0.3)
                } else {
                    Color::BLACK.with_alpha(0.3)
                })
                .border(1.0)
                .width(12.0)
                .height(12.0)
                .margin_left(offset)
                .margin_right(4.0)
                .border_color(Color::BLACK.with_alpha(0.4))
                .border_radius(4.0)
                .hover(move |s| {
                    s.border_color(Color::BLACK.with_alpha(0.6))
                        .background(if expanded.get() {
                            Color::WHITE.with_alpha(0.5)
                        } else {
                            Color::BLACK.with_alpha(0.5)
                        })
                })
            })
            .on_click(move |_| {
                expanded.set(!expanded.get());
            }),
        name,
    ))
    .style(move |s| {
        s.padding_left(3.0)
            .items_center()
            .hover(move |s| {
                s.background(Color::rgba8(228, 237, 216, 160))
                    .apply_if(selected.get() == Some(id), |s| {
                        s.background(Color::rgb8(186, 180, 216))
                    })
            })
            .height(height)
            .apply_if(highlighted.get() == Some(id), |s| {
                s.background(Color::rgba8(228, 237, 216, 160))
            })
            .apply_if(selected.get() == Some(id), |s| {
                if highlighted.get() == Some(id) {
                    s.background(Color::rgb8(186, 180, 216))
                } else {
                    s.background(Color::rgb8(213, 208, 216))
                }
            })
    })
    .on_click(move |_| selected.set(Some(id)))
    .on_enter(move || highlighted.set(Some(id)));

    let row_id = row.id();
    let scroll_to = capture_view.scroll_to;
    create_effect(move || {
        if let Some(selection) = expanding_selection.get() {
            if selection != id && view_.find(selection).is_some() {
                expanded.set(true);
            }
            if selection == id {
                // Scroll to the row, then to the name part of the row.
                scroll_to.set(Some(row_id));
                scroll_to.set(Some(name_id));
            }
        }
    });

    let child_count = children.len();

    let line = empty().style(move |s| {
        let line = if expanded.get() {
            child_count as f64 * 20.0
        } else {
            0.0
        };
        s.absolute()
            .height(line)
            .width(1.0)
            .margin_left(9.0 + offset)
            .background(Color::BLACK.with_alpha(0.1))
    });
    let line = h_stack((empty().style(move |s| s.width(5.0 + offset)), line));

    let list = v_stack_from_iter(children).style(move |s| s.display(expanded.get()).items_start());

    let list = z_stack_from_iter([
        (line.any(), WidgetId::next(), Point::ZERO),
        (list.any(), WidgetId::next(), Point::ZERO),
    ]);

    v_stack((row, list)).style(|s| s.items_start()).any()
}

fn captured_view(view: &Rc<CapturedView>, depth: usize, capture_view: &CaptureView) -> AnyView {
    if view.children.is_empty() {
        captured_view_no_children(view, depth, capture_view)
    } else {
        let children: Vec<_> = view
            .children
            .iter()
            .map(|view| captured_view(view, depth + 1, capture_view))
            .collect();
        captured_view_with_children(view, depth, capture_view, children)
    }
}

pub(crate) fn header(label: impl Display) -> View<impl Widget> {
    text(label).style(|s| {
        s.padding(5.0)
            .background(WHITE_SMOKE)
            .width_full()
            .height(27.0)
            .border_bottom(1.0)
            .border_color(LIGHT_GRAY)
    })
}

fn info(name: impl Display, value: String) -> View<impl Widget> {
    info_row(name.to_string(), text(value))
}

fn info_row(name: String, view: View<impl Any>) -> View<impl Widget> {
    h_stack((
        container(text(name).style(|s| s.margin_right(5.0).color(Color::BLACK.with_alpha(0.6))))
            .style(|s| s.min_width(150.0).flex_row_reverse()),
        view,
    ))
    .style(|s| {
        s.padding(5.0)
            .hover(|s| s.background(Color::rgba8(228, 237, 216, 160)))
    })
}

fn stats(capture: &Capture) -> View<impl Widget> {
    let layout_time = capture.post_layout.saturating_duration_since(capture.start);
    let paint_time = capture.end.saturating_duration_since(capture.post_layout);
    let layout_time = info(
        "Layout Time",
        format!("{:.4} ms", layout_time.as_secs_f64() * 1000.0),
    );
    let paint_time = info(
        "Paint Time",
        format!("{:.4} ms", paint_time.as_secs_f64() * 1000.0),
    );
    let w = info("Window Width", format!("{}", capture.window_size.width));
    let h = info("Window Height", format!("{}", capture.window_size.height));
    v_stack((layout_time, paint_time, w, h))
}

fn selected_view(capture: &Rc<Capture>, selected: RwSignal<Option<Id>>) -> AnyView {
    let capture = capture.clone();
    dyn_container(
        move || selected.get(),
        move |current| {
            if let Some(view) = current.and_then(|id| capture.root.find(id)) {
                let name = info("Type", view.name.clone());
                let id = info("Id", view.id.to_raw().to_string());
                let count = info("Child Count", format!("{}", view.children.len()));
                let beyond = |view: f64, window| {
                    if view > window {
                        format!(" ({} after window edge)", view - window)
                    } else if view < 0.0 {
                        format!(" ({} before window edge)", -view)
                    } else {
                        String::new()
                    }
                };
                let x = info(
                    "X",
                    format!(
                        "{}{}",
                        view.layout.x0,
                        beyond(view.layout.x0, capture.window_size.width)
                    ),
                );
                let y = info(
                    "Y",
                    format!(
                        "{}{}",
                        view.layout.y0,
                        beyond(view.layout.y0, capture.window_size.height)
                    ),
                );
                let w = info(
                    "Width",
                    format!(
                        "{}{}",
                        view.layout.width(),
                        beyond(view.layout.x1, capture.window_size.width)
                    ),
                );
                let h = info(
                    "Height",
                    format!(
                        "{}{}",
                        view.layout.height(),
                        beyond(view.layout.y1, capture.window_size.height)
                    ),
                );
                let clear = button(|| "Clear selection")
                    .style(|s| s.margin(5.0))
                    .on_click(move |_| selected.set(None));
                let clear = container(clear);

                v_stack((name, id, count, x, y, w, h, clear))
                    .style(|s| s.width_full())
                    .any()
            } else {
                text("No selection").style(|s| s.padding(5.0)).any()
            }
        },
    )
    .any()
}

#[derive(Clone, Copy)]
struct CaptureView {
    expanding_selection: RwSignal<Option<Id>>,
    scroll_to: RwSignal<Option<Id>>,
    selected: RwSignal<Option<Id>>,
    highlighted: RwSignal<Option<Id>>,
}

fn capture_view(capture: &Rc<Capture>, widget: WidgetPod<Box<dyn Widget>>) -> View<impl Widget> {
    let capture_view = CaptureView {
        expanding_selection: create_rw_signal(None),
        scroll_to: create_rw_signal(None),
        selected: create_rw_signal(None),
        highlighted: create_rw_signal(None),
    };

    let capture__ = capture.clone();
    let window_size = capture.window_size;

    let image = View::new(
        SizedBox::new(DisplayRoot {
            root: widget,
            size: window_size,
        })
        .height(window_size.height)
        .width(window_size.width)
        .background(BackgroundBrush::Color(capture.background.into())),
    );
    let capture_ = capture.clone();
    let selected_overlay_id = WidgetId::next();
    let selected_overlay = empty().style(move |s| {
        if let Some(view) = capture_view
            .selected
            .get()
            .and_then(|id| capture_.root.find(id))
        {
            s.width(view.layout.width())
                .height(view.layout.height())
                .background(Color::rgb8(186, 180, 216).with_alpha(0.5))
                .border_color(Color::rgb8(186, 180, 216).with_alpha(0.7))
                .border(1.0)
        } else {
            s
        }
    });

    let capture_ = capture.clone();
    let highlighted_overlay_id = WidgetId::next();
    let highlighted_overlay = empty().style(move |s| {
        if let Some(view) = capture_view
            .highlighted
            .get()
            .and_then(|id| capture_.root.find(id))
        {
            s.width(view.layout.width())
                .height(view.layout.height())
                .background(Color::rgba8(228, 237, 216, 120))
                .border_color(Color::rgba8(75, 87, 53, 120))
                .border(1.0)
        } else {
            s
        }
    });

    let capture_ = capture.clone();
    let image = z_stack_from_iter([
        (image.any(), WidgetId::next(), Point::ZERO),
        (selected_overlay.any(), selected_overlay_id, Point::ZERO),
        (
            highlighted_overlay.any(),
            highlighted_overlay_id,
            Point::ZERO,
        ),
    ])
    .style(|s| {
        s.margin(5.0)
            .border(1.0)
            .border_color(Color::BLACK.with_alpha(0.5))
            .margin_bottom(21.0)
            .margin_right(21.0)
    })
    .on_any_event(move |e| {
        if let Event::MouseMove(e) = e {
            if let Some(view) = capture_.root.find_by_pos(e.pos) {
                if capture_view.highlighted.get() != Some(view.id) {
                    capture_view.highlighted.set(Some(view.id));
                }
            } else if capture_view.highlighted.get().is_some() {
                capture_view.highlighted.set(None);
            }
        }
    })
    .on_click(move |e| {
        if let Event::MouseDown(e) = e {
            if let Some(view) = capture__.root.find_by_pos(e.pos) {
                capture_view.selected.set(Some(view.id));
                capture_view.expanding_selection.set(Some(view.id));
                return;
            }
            if capture_view.selected.get().is_some() {
                capture_view.selected.set(None);
            }
        }
    })
    .on_leave(move || capture_view.highlighted.set(None));

    let image_stack_id = image.widget_id();

    let capture_ = capture.clone();
    create_effect(move || {
        if let Some(view) = capture_view
            .selected
            .get()
            .and_then(|id| capture_.root.find(id))
        {
            let position = view.layout.origin();
            update_widget::<Stack>(image_stack_id, move |mut stack| {
                stack.set_child_position(selected_overlay_id, position)
            });
        }
    });

    let capture_ = capture.clone();
    create_effect(move || {
        if let Some(view) = capture_view
            .highlighted
            .get()
            .and_then(|id| capture_.root.find(id))
        {
            let position = view.layout.origin();
            update_widget::<Stack>(image_stack_id, move |mut stack| {
                stack.set_child_position(highlighted_overlay_id, position)
            });
        }
    });

    let window_id = capture.window_id;

    let left_scroll = scroll(
        v_stack((
            header("Selected View"),
            selected_view(capture, capture_view.selected),
            header("Stats"),
            stats(capture),
            button(|| "Recursive Inspection").on_click(move |_| {
                CURRENT_RUNTIME.with(|runtime| {
                    runtime.push_command(Command::new(INSPECT, (), Target::Window(window_id)));
                })
            }),
        ))
        .style(|s| s.min_width_full()),
    )
    .style(|s| {
        s.width_full()
            .flex_basis(0)
            .min_height(0.0)
            .flex_grow(1.0)
            .flex_col()
    });

    let seperator = empty().style(move |s| {
        s.width_full()
            .min_height(1.0)
            .background(Color::BLACK.with_alpha(0.2))
    });

    let left = v_stack((
        header("Captured Window"),
        scroll(image).style(|s| s.max_height_pct(60.0)),
        seperator,
        left_scroll,
    ))
    .style(|s| s.max_width_pct(60.0).items_start());

    let tree = scroll(captured_view(&capture.root, 0, &capture_view).style(|s| s.min_width_full()))
        .style(|s| {
            s.width_full()
                .min_height(0)
                .flex_basis(0)
                .flex_grow(1.0)
                .flex_col()
                .force_height_full()
        })
        .on_leave(move || capture_view.highlighted.set(None))
        .on_click(move |_| capture_view.selected.set(None))
        .scroll_to_view(move || capture_view.scroll_to.get())
        .grow();

    let tree = if capture.root.warnings() {
        v_stack((header("Warnings"), header("View Tree"), tree))
            .style(|s| s.items_start())
            .any()
    } else {
        v_stack((header("View Tree"), tree))
            .style(|s| s.items_start())
            .any()
    };

    let tree = tree
        .style(|s| {
            s.height_full()
                .min_width(0)
                .flex_basis(0)
                .flex_grow(1.0)
                .force_width_full()
        })
        .grow();

    let seperator = empty().style(move |s| {
        s.height_full()
            .min_width(1.0)
            .width(1.0)
            .background(Color::BLACK.with_alpha(0.2))
    });

    h_stack((left, seperator, tree))
        .style(|s| s.height_full().width_full().max_width_full().items_start())
}

fn inspector_view(
    capture: &Option<Rc<Capture>>,
    widget: WidgetPod<Box<dyn Widget>>,
) -> View<impl Widget> {
    let view = if let Some(capture) = capture {
        capture_view(capture, widget).any()
    } else {
        text("No capture").any()
    };

    container(view).style(|s| s.width_full().height_full().background(Color::WHITE))
}

pub struct DisplayRoot {
    root: WidgetPod<Box<dyn Widget>>,
    size: Size,
}

impl Widget for DisplayRoot {
    fn on_event(&mut self, _ctx: &mut EventCtx, _event: &Event, _env: &Env) {}

    fn on_status_change(&mut self, _ctx: &mut LifeCycleCtx, _event: &StatusChange, _env: &Env) {}

    fn lifecycle(&mut self, _ctx: &mut LifeCycleCtx, _event: &LifeCycle, _env: &Env) {}

    fn layout(&mut self, ctx: &mut LayoutCtx, _bc: &BoxConstraints, env: &Env) -> Size {
        let size = self
            .root
            .layout(ctx, &BoxConstraints::tight(self.size), env);
        ctx.place_child(&mut self.root, Point::ZERO, env);
        size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        self.root.paint(ctx, env);
    }

    fn children(&self) -> SmallVec<[WidgetRef<'_, dyn Widget>; 16]> {
        SmallVec::new()
    }
}

pub fn inspect(
    widget: WidgetPod<Box<dyn Widget>>,
    window_size: Size,
    background: Color,
    command_queue: &mut CommandQueue,
    window_id: WindowId,
) -> impl Widget {
    let root = CapturedView::capture(widget.as_dyn(), window_size.to_rect());
    let now = Instant::now();
    let capture = Capture {
        start: now,
        post_layout: now,
        end: now,
        window_size,
        window_id,
        root: Rc::new(root),
        background,
    };
    RuntimeView::new(command_queue, || {
        inspector_view(&Some(Rc::new(capture)), widget)
    })
}
