use crate::command::CommandQueue;
use crate::widget::StoreInWidgetMut;
use crate::widget::WidgetMut;
use crate::widget::WidgetRef;
use crate::BoxConstraints;
use crate::Command;
use crate::Env;
use crate::Event;
use crate::EventCtx;
use crate::LayoutCtx;
use crate::LifeCycle;
use crate::LifeCycleCtx;
use crate::PaintCtx;
use crate::Selector;
use crate::StatusChange;
use crate::Target;
use crate::Widget;
use crate::WidgetCtx;
use crate::WidgetId;
use crate::WidgetPod;
use crate::WidgetState;
use piet_common::kurbo::Point;
use piet_common::kurbo::Size;
use scoped_tls::scoped_thread_local;
use slotmap::new_key_type;
use slotmap::SlotMap;
use smallvec::smallvec;
use smallvec::SmallVec;
use std::any::type_name;
use std::cell::Cell;
use std::collections::HashMap;
use std::{any::Any, cell::RefCell, marker::PhantomData, rc::Rc};
use tracing::trace_span;
use tracing::Span;

new_key_type! { struct SignalKey; }

pub struct Runtime {
    next_effect_id: Cell<u64>,
    signals: RefCell<SlotMap<SignalKey, Signal>>,
    current_effect: RefCell<Option<Rc<Effect>>>,
    command_queue: RefCell<Vec<Command>>,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            next_effect_id: Cell::new(0),
            current_effect: RefCell::new(None),
            signals: Default::default(),
            command_queue: Default::default(),
        }
    }

    pub fn push_command(&self, command: Command) {
        self.command_queue.borrow_mut().push(command)
    }

    /// Forward queued commands to the real command queue.
    fn flush_commands(&self, command_queue: &mut CommandQueue) {
        command_queue.extend(self.command_queue.borrow_mut().drain(..))
    }

    fn as_current<R>(&self, f: impl FnOnce() -> R) -> R {
        CURRENT_RUNTIME.set(self, f)
    }
}

scoped_thread_local!(pub(crate) static CURRENT_RUNTIME: Runtime);

pub struct RuntimeView {
    runtime: Runtime,
    widget: WidgetPod<Box<dyn Widget>>,
}

impl RuntimeView {
    pub fn new<W: Widget>(command_queue: &mut CommandQueue, make_view: impl FnOnce() -> W) -> Self {
        let runtime = Runtime::new();
        let widget = Box::new(runtime.as_current(make_view));
        runtime.flush_commands(command_queue);
        Self {
            runtime,
            widget: WidgetPod::new(widget),
        }
    }
}

impl Widget for RuntimeView {
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) {
        self.runtime
            .as_current(|| self.widget.on_event(ctx, event, env));
        self.runtime.flush_commands(ctx.global_state.command_queue);
    }

    fn on_status_change(&mut self, _ctx: &mut LifeCycleCtx, _event: &StatusChange, _env: &Env) {}

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env) {
        self.widget.lifecycle(ctx, event, env);
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size {
        let size = self.runtime.as_current(|| {
            let size = self.widget.layout(ctx, bc, env);
            ctx.place_child(&mut self.widget, Point::ZERO, env);
            size
        });
        self.runtime.flush_commands(ctx.global_state.command_queue);
        size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        self.widget.paint(ctx, env);
    }

    fn children(&self) -> SmallVec<[WidgetRef<'_, dyn Widget>; 16]> {
        smallvec![self.widget.as_dyn()]
    }

    fn make_trace_span(&self) -> Span {
        trace_span!("RuntimeView")
    }
}

type UpdaterFn = Box<dyn FnOnce(UpdateWidgetArgs<'_, '_>)>;

pub(crate) const UPDATE_WIDGET: Selector<RefCell<Option<UpdaterFn>>> =
    Selector::new("masonry-builtin.mini.update-widget");

pub(crate) struct UpdateWidgetArgs<'a, 'b> {
    pub(crate) type_name: &'static str,
    pub(crate) widget: &'a mut dyn Any,
    pub(crate) parent_state: &'a mut WidgetState,
    pub(crate) ctx: WidgetCtx<'a, 'b>,
}

pub fn update_widget_state(widget_id: WidgetId, f: impl FnOnce(&mut WidgetState) + 'static) {
    CURRENT_RUNTIME.with(|runtime| {
        let updater: RefCell<Option<UpdaterFn>> = RefCell::new(Some(Box::new(move |args| {
            f(args.ctx.widget_state);
        })));
        runtime.push_command(Command::new(
            UPDATE_WIDGET,
            updater,
            Target::Widget(widget_id),
        ));
    })
}

pub fn update_widget<W: Widget + StoreInWidgetMut>(
    widget_id: WidgetId,
    f: impl FnOnce(WidgetMut<'_, '_, W>) + 'static,
) {
    CURRENT_RUNTIME.with(|runtime| {
        let updater: RefCell<Option<UpdaterFn>> = RefCell::new(Some(Box::new(move |args| {
            let widget = args.widget.downcast_mut::<W>().unwrap_or_else(|| {
                panic!(
                    "expected to update widget with type `{}`, but found `{}` in the widget tree",
                    type_name::<W>(),
                    args.type_name
                )
            });
            let widget_mut = WidgetMut {
                parent_widget_state: args.parent_state,
                inner: W::from_widget_and_ctx(widget, args.ctx),
            };

            f(widget_mut);
        })));
        runtime.push_command(Command::new(
            UPDATE_WIDGET,
            updater,
            Target::Widget(widget_id),
        ));
    })
}

#[derive(Hash, Copy, Clone, Eq, PartialEq)]
struct EffectId(u64);

struct Signal {
    value: Box<dyn Any>,
    subscribers: HashMap<EffectId, Rc<Effect>>,
}

pub struct RwSignal<T: 'static> {
    key: SignalKey,
    phantom: PhantomData<T>,
}

impl<T> Copy for RwSignal<T> {}

impl<T> Clone for RwSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone> RwSignal<T> {
    pub fn set(self, new_value: T) {
        CURRENT_RUNTIME.with(|runtime| {
            let subscribers: Vec<Rc<Effect>> = {
                let mut signals = runtime.signals.borrow_mut();
                let signal = signals.get_mut(self.key).unwrap();

                *signal.value.downcast_mut::<T>().unwrap() = new_value;

                signal.subscribers.values().cloned().collect()
            };

            for subscriber in subscribers {
                subscriber.run(runtime);
            }
        })
    }

    pub fn get(self) -> T {
        CURRENT_RUNTIME.with(|runtime| {
            let effect = runtime.current_effect.borrow().clone();
            let mut signals = runtime.signals.borrow_mut();
            let signal = signals.get_mut(self.key).unwrap();
            if let Some(effect) = effect {
                effect.observers.borrow_mut().push(self.key);
                signal.subscribers.insert(effect.id, effect);
            }
            signal.value.downcast_ref::<T>().unwrap().clone()
        })
    }
}

pub fn create_rw_signal<T>(value: T) -> RwSignal<T> {
    CURRENT_RUNTIME.with(|runtime| RwSignal {
        key: runtime.signals.borrow_mut().insert(Signal {
            value: Box::new(value),
            subscribers: HashMap::default(),
        }),
        phantom: PhantomData,
    })
}

struct Effect {
    id: EffectId,
    run: Box<dyn Fn()>,
    observers: RefCell<Vec<SignalKey>>,
}

impl Effect {
    fn run(self: &Rc<Self>, runtime: &Runtime) {
        // Remove the effect from all signals which subscribe to it.
        {
            let mut signals = runtime.signals.borrow_mut();
            for observer in self.observers.borrow_mut().drain(..) {
                if let Some(signal) = signals.get_mut(observer) {
                    signal.subscribers.remove(&self.id);
                }
            }
        }
        *runtime.current_effect.borrow_mut() = Some(self.clone());
        (self.run)();
        *runtime.current_effect.borrow_mut() = None;
    }
}

pub fn create_effect(f: impl Fn() + 'static) {
    CURRENT_RUNTIME.with(|runtime| {
        let id = runtime.next_effect_id.get();
        runtime.next_effect_id.set(id + 1);

        let effect = Rc::new(Effect {
            id: EffectId(id),
            run: Box::new(f),
            observers: RefCell::new(Vec::new()),
        });
        effect.run(runtime);
    })
}

pub fn untrack<R>(f: impl FnOnce() -> R) -> R {
    CURRENT_RUNTIME.with(|runtime| {
        let old = runtime.current_effect.borrow_mut().take();
        let r = f();
        *runtime.current_effect.borrow_mut() = old;
        r
    })
}
