use super::reactive::{create_effect, update_widget};
use super::view::View;
use crate::widget::CrossAxisAlignment;
use crate::{
    widget::{Flex, Label, SizedBox},
    BackgroundBrush, KeyOrValue,
};
use piet_common::Color;
use std::f64::INFINITY;

pub const WHITE_SMOKE: Color = Color::rgba8(245, 245, 245, 255);
pub const LIGHT_GRAY: Color = Color::rgba8(211, 211, 211, 255);

#[derive(Default, Clone)]
pub struct Style {
    color: Option<Color>,
    font_size: Option<f64>,
    background: Option<Color>,
    width: Option<f64>,
    height: Option<f64>,
    border: Option<f64>,
    border_color: Option<Color>,
    border_radius: Option<f64>,
    grow: bool,
    hidden: bool,
    cross_axis_alignment: Option<CrossAxisAlignment>,
}

impl Style {
    pub fn absolute(self) -> Self {
        self
    }

    pub fn display(mut self, visible: bool) -> Self {
        self.hidden = !visible;
        self
    }

    pub fn flex_row_reverse(self) -> Self {
        self
    }

    pub fn flex_col(self) -> Self {
        self
    }

    pub fn flex_basis(self, _v: usize) -> Self {
        self
    }

    pub fn flex_grow(mut self, _v: f64) -> Self {
        self.grow = true;
        self
    }

    pub fn font_size(mut self, v: f64) -> Self {
        self.font_size = Some(v);
        self
    }

    pub fn height_full(self) -> Self {
        //self.height = Some(INFINITY);
        self
    }

    pub fn force_height_full(mut self) -> Self {
        self.height = Some(INFINITY);
        self
    }

    pub fn width_full(self) -> Self {
        //self.width = Some(INFINITY);
        self
    }

    pub fn force_width_full(mut self) -> Self {
        self.width = Some(INFINITY);
        self
    }

    pub fn min_width(self, _v: impl Into<f64>) -> Self {
        self
    }

    pub fn min_width_full(self) -> Self {
        self
    }

    pub fn max_width_full(self) -> Self {
        self
    }

    pub fn max_width_pct(self, _v: impl Into<f64>) -> Self {
        self
    }

    pub fn width(mut self, v: f64) -> Self {
        self.width = Some(v);
        self
    }

    pub fn min_height(self, _v: impl Into<f64>) -> Self {
        self
    }

    pub fn max_height_pct(self, _v: impl Into<f64>) -> Self {
        self
    }

    pub fn height(mut self, v: f64) -> Self {
        self.height = Some(v);
        self
    }

    pub fn padding(self, _v: f64) -> Self {
        self
    }

    pub fn padding_left(self, _v: f64) -> Self {
        self
    }

    #[allow(unused)]
    pub fn padding_right(self, _v: f64) -> Self {
        self
    }

    pub fn padding_top(self, _v: f64) -> Self {
        self
    }

    pub fn padding_bottom(self, _v: f64) -> Self {
        self
    }

    pub fn margin(self, _v: f64) -> Self {
        self
    }

    pub fn margin_left(self, _v: f64) -> Self {
        self
    }

    pub fn margin_right(self, _v: f64) -> Self {
        self
    }

    #[allow(unused)]
    pub fn margin_top(self, _v: f64) -> Self {
        self
    }

    pub fn margin_bottom(self, _v: f64) -> Self {
        self
    }

    pub fn items_start(mut self) -> Self {
        self.cross_axis_alignment = Some(CrossAxisAlignment::Start);
        self
    }

    pub fn items_center(mut self) -> Self {
        self.cross_axis_alignment = Some(CrossAxisAlignment::Center);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn border(mut self, v: f64) -> Self {
        self.border = Some(v);
        self
    }

    pub fn border_bottom(self, _v: f64) -> Self {
        self
    }

    pub fn border_radius(mut self, v: f64) -> Self {
        self.border_radius = Some(v);
        self
    }

    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = Some(color);
        self
    }

    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond {
            f(self)
        } else {
            self
        }
    }

    pub fn hover(self, _style: impl FnOnce(Style) -> Style) -> Self {
        self
    }
}

impl<W> View<W> {
    pub fn style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move || {
            let style = style(Style::default());
            update_widget::<SizedBox>(id, move |mut this| {
                this.clear_background();
                this.clear_border();
                this.unset_width();
                this.unset_height();

                if let Some(background) = style.background {
                    this.set_background(BackgroundBrush::Color(KeyOrValue::Concrete(background)));
                }
                this.set_visible(!style.hidden);
                if let Some(width) = style.width {
                    this.set_width(width);
                }
                if let Some(height) = style.height {
                    this.set_height(height);
                }
                if let Some(width) = style.border {
                    let color = style.border_color.unwrap_or(Color::BLACK);
                    this.set_border(color, width);
                }
                this.set_rounded(style.border_radius.unwrap_or(0.0));

                if let Some(color) = style.color {
                    if let Some(mut label) = this.child_mut().unwrap().downcast::<Label>() {
                        label.set_text_color(color)
                    }
                }
                if let Some(font_size) = style.font_size {
                    if let Some(mut label) = this.child_mut().unwrap().downcast::<Label>() {
                        label.set_text_size(font_size);
                    }
                }
                if let Some(cross_axis_alignment) = style.cross_axis_alignment {
                    if let Some(mut flex) = this.child_mut().unwrap().downcast::<Flex>() {
                        flex.set_cross_axis_alignment(cross_axis_alignment);
                    }
                }
            });
        });
        self
    }
}
