
use egui::{Align2, Color32, FontId, ImageSource, Rect, Shape, Stroke, Ui, include_image, pos2, vec2};
use gilrs::Button;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Layout {
    Xbox,
    PlayStation,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default, Debug)]
pub enum Prompts {
    #[default]
    Auto,
    Xbox,
    PlayStation,
}

impl Prompts {
    pub fn next(self, forward: bool) -> Self {
        match (self, forward) {
            (Prompts::Auto, true) | (Prompts::PlayStation, false) => Prompts::Xbox,
            (Prompts::Xbox, true) | (Prompts::Auto, false) => Prompts::PlayStation,
            (Prompts::PlayStation, true) | (Prompts::Xbox, false) => Prompts::Auto,
        }
    }

    pub fn layout(self, detected: Layout) -> Layout {
        match self {
            Prompts::Auto => detected,
            Prompts::Xbox => Layout::Xbox,
            Prompts::PlayStation => Layout::PlayStation,
        }
    }
}

const GRAY: Color32 = Color32::from_gray(90);

pub fn name(button: Button, layout: Layout) -> &'static str {
    let (xbox, ps) = match button {
        Button::South => ("A", "Cross"),
        Button::East => ("B", "Circle"),
        Button::West => ("X", "Square"),
        Button::North => ("Y", "Triangle"),
        Button::LeftTrigger => ("LB", "L1"),
        Button::RightTrigger => ("RB", "R1"),
        Button::LeftTrigger2 => ("LT", "L2"),
        Button::RightTrigger2 => ("RT", "R2"),
        Button::Select => ("View", "Create"),
        Button::Start => ("Menu", "Options"),
        Button::Mode => ("Guide", "PS"),
        Button::DPadLeft => ("◀", "◀"),
        Button::DPadRight => ("▶", "▶"),
        Button::DPadUp => ("▲", "▲"),
        Button::DPadDown => ("▼", "▼"),
        _ => ("?", "?"),
    };
    if layout == Layout::Xbox { xbox } else { ps }
}

pub fn size(ui: &Ui, button: Button, layout: Layout, images: bool, s: f32) -> egui::Vec2 {
    if images && image(button, layout).is_some() {
        return vec2(40.0, 40.0) * s;
    }
    if layout == Layout::PlayStation && face(button) {
        return vec2(28.0, 28.0) * s;
    }
    let text = ui.painter().layout_no_wrap(name(button, layout).to_owned(), font(s), Color32::WHITE).size().x;
    vec2((text + 14.0 * s).max(28.0 * s), 28.0 * s)
}

pub fn paint(ui: &Ui, rect: Rect, button: Button, layout: Layout, images: bool, hot: bool, s: f32) {
    let painter = ui.painter();
    if images && let Some(source) = image(button, layout) {
        if hot {
            painter.rect_filled(rect, (8.0 * s) as u8, Color32::from_white_alpha(40));
        }
        egui::Image::new(source).paint_at(ui, rect);
        return;
    }
    let color = color(button, layout);
    let fill = if hot { color.lerp_to_gamma(Color32::WHITE, 0.3) } else { color };
    painter.rect_filled(rect, (14.0 * s) as u8, fill);
    if layout == Layout::PlayStation && face(button) {
        return paint_symbol(painter, rect, button, s);
    }
    painter.text(rect.center(), Align2::CENTER_CENTER, name(button, layout), font(s), Color32::WHITE);
}

fn font(s: f32) -> FontId {
    FontId::proportional(14.0 * s)
}

fn face(button: Button) -> bool {
    matches!(button, Button::South | Button::East | Button::West | Button::North)
}

fn color(button: Button, layout: Layout) -> Color32 {
    match (layout, button) {
        (Layout::Xbox, Button::South) => Color32::from_rgb(0x5c, 0xb8, 0x3c),
        (Layout::Xbox, Button::East) => Color32::from_rgb(0xd9, 0x3a, 0x3a),
        (Layout::Xbox, Button::West) => Color32::from_rgb(0x2f, 0x7d, 0xf6),
        (Layout::Xbox, Button::North) => Color32::from_rgb(0xe8, 0xb9, 0x24),
        (Layout::PlayStation, Button::South) => Color32::from_rgb(0x6f, 0x8f, 0xe8),
        (Layout::PlayStation, Button::East) => Color32::from_rgb(0xe0, 0x4a, 0x5a),
        (Layout::PlayStation, Button::West) => Color32::from_rgb(0xd0, 0x6f, 0xc0),
        (Layout::PlayStation, Button::North) => Color32::from_rgb(0x2f, 0xb8, 0x9a),
        _ => GRAY,
    }
}

fn paint_symbol(painter: &egui::Painter, rect: Rect, button: Button, s: f32) {
    let c = rect.center();
    let r = 6.5 * s;
    let stroke = Stroke::new(2.0 * s, Color32::WHITE);
    match button {
        Button::South => {
            painter.line_segment([c + vec2(-r, -r), c + vec2(r, r)], stroke);
            painter.line_segment([c + vec2(-r, r), c + vec2(r, -r)], stroke);
        }
        Button::East => {
            painter.circle_stroke(c, r, stroke);
        }
        Button::West => {
            painter.rect_stroke(Rect::from_center_size(c, vec2(2.0 * r, 2.0 * r) * 0.9), 0, stroke, egui::StrokeKind::Middle);
        }
        _ => {
            let points = vec![pos2(c.x, c.y - r), pos2(c.x + r * 1.1, c.y + r * 0.8), pos2(c.x - r * 1.1, c.y + r * 0.8)];
            painter.add(Shape::closed_line(points, stroke));
        }
    }
}

macro_rules! xbox {
    ($name:literal) => {
        include_image!(concat!("../assets/buttons_prompt_images/Xbox_Series/Default/", $name, ".png"))
    };
}

macro_rules! playstation {
    ($name:literal) => {
        include_image!(concat!("../assets/buttons_prompt_images/PlayStation_Series/Default/", $name, ".png"))
    };
}

fn image(button: Button, layout: Layout) -> Option<ImageSource<'static>> {
    Some(match layout {
        Layout::Xbox => match button {
            Button::South => xbox!("xbox_button_color_a"),
            Button::East => xbox!("xbox_button_color_b"),
            Button::West => xbox!("xbox_button_color_x"),
            Button::North => xbox!("xbox_button_color_y"),
            Button::LeftTrigger => xbox!("xbox_lb"),
            Button::RightTrigger => xbox!("xbox_rb"),
            Button::LeftTrigger2 => xbox!("xbox_lt"),
            Button::RightTrigger2 => xbox!("xbox_rt"),
            Button::Select => xbox!("xbox_button_view"),
            Button::Start => xbox!("xbox_button_menu"),
            Button::Mode => xbox!("xbox_guide"),
            Button::DPadLeft => xbox!("xbox_dpad_left"),
            Button::DPadRight => xbox!("xbox_dpad_right"),
            Button::DPadUp => xbox!("xbox_dpad_up"),
            Button::DPadDown => xbox!("xbox_dpad_down"),
            _ => return None,
        },
        Layout::PlayStation => match button {
            Button::South => playstation!("playstation_button_color_cross"),
            Button::East => playstation!("playstation_button_color_circle"),
            Button::West => playstation!("playstation_button_color_square"),
            Button::North => playstation!("playstation_button_color_triangle"),
            Button::LeftTrigger => playstation!("playstation_trigger_l1"),
            Button::RightTrigger => playstation!("playstation_trigger_r1"),
            Button::LeftTrigger2 => playstation!("playstation_trigger_l2"),
            Button::RightTrigger2 => playstation!("playstation_trigger_r2"),
            Button::Select => playstation!("playstation5_button_create"),
            Button::Start => playstation!("playstation5_button_options"),
            Button::DPadLeft => playstation!("playstation_dpad_left"),
            Button::DPadRight => playstation!("playstation_dpad_right"),
            Button::DPadUp => playstation!("playstation_dpad_up"),
            Button::DPadDown => playstation!("playstation_dpad_down"),
            _ => return None,
        },
    })
}
