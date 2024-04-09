// This software is licensed under Apache License 2.0 and distributed on an
// "as-is" basis without warranties of any kind. See the LICENSE file for
// details.

//! This showcase demonstrates how to use the image widget and is
//! propperties. You can change the parameters in the GUI to see how
//! everything behaves.

// On Windows platform, don't show a console when opening the app.
#![windows_subsystem = "windows"]

use masonry::widget::{FillStrat, Image};
use masonry::{AppLauncher, WindowDescription};
use vello::peniko::{Format, Image as ImageBuf};

pub fn main() {
    let image_bytes = include_bytes!("./assets/PicWithAlpha.png");
    let image_data = image::load_from_memory(image_bytes).unwrap().to_rgba8();
    let (width, height) = image_data.dimensions();
    let png_data = ImageBuf::new(image_data.to_vec().into(), Format::Rgba8, width, height);
    let image = Image::new(png_data).fill_mode(FillStrat::Contain);

    let main_window = WindowDescription::new(image)
        .window_size((650., 450.))
        .title("Simple image example");

    AppLauncher::with_window(main_window)
        .log_to_console()
        .launch()
        .expect("Failed to launch application");
}
