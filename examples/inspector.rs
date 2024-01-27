// This software is licensed under Apache License 2.0 and distributed on an
// "as-is" basis without warranties of any kind. See the LICENSE file for
// details.

//! This is a very small example of how to use the widget inspector.

// On Windows platform, don't show a console when opening the app.
#![windows_subsystem = "windows"]

use masonry::command::INSPECT;
use masonry::widget::{prelude::*, SizedBox};
use masonry::widget::{Button, Flex, Label};
use masonry::{Action, Target};
use masonry::{AppDelegate, DelegateCtx};
use masonry::{AppLauncher, WindowDescription, WindowId};

const VERTICAL_WIDGET_SPACING: f64 = 20.0;

struct Delegate;

impl AppDelegate for Delegate {
    fn on_action(
        &mut self,
        _ctx: &mut DelegateCtx,
        _window_id: WindowId,
        _widget_id: WidgetId,
        action: Action,
        _env: &Env,
    ) {
        if let Action::ButtonPressed = action {
            _ctx.get_external_handle()
                .submit_command(INSPECT, Box::new(()), Target::Window(_window_id))
                .unwrap();
        }
    }
}

pub fn main() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            // describe the main window
            let main_window = WindowDescription::new(build_root_widget())
                .title("Hello World!")
                .window_size((600.0, 400.0));

            // start the application. Here we pass in the application state.
            AppLauncher::with_window(main_window)
                .with_delegate(Delegate)
                .log_to_console()
                .launch()
                .expect("Failed to launch application");
        })
        .unwrap()
        .join()
        .unwrap();
}

fn build_root_widget() -> impl Widget {
    let label = Label::new("Hello").with_text_size(32.0);

    // a button that says "Inspect"
    let button = SizedBox::new(Button::new("Inspect"));

    // arrange the two widgets vertically, with some padding
    Flex::column()
        .with_child(label)
        .with_spacer(VERTICAL_WIDGET_SPACING)
        .with_child(button)
}
