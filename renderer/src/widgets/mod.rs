pub mod clock_dial;
pub mod common;
pub mod ha;
pub mod placeholder;

use serde_json::Value;
use skia_safe::Rect;

use crate::canvas::Canvas2D;
use crate::data::Need;
use crate::layout::Widget;
use crate::theme::Theme;

pub const CELL: f32 = 160.0;

pub fn widget_rect(w: &Widget) -> Rect {
    Rect::from_xywh(
        w.cell[0] as f32 * CELL,
        w.cell[1] as f32 * CELL,
        w.cell[2] as f32 * CELL,
        w.cell[3] as f32 * CELL,
    )
}

pub fn needs(w: &Widget) -> Vec<Need> {
    match w.kind.as_str() {
        "clock_dial" => clock_dial::needs(&w.config),
        "ha" => ha::needs(&w.config),
        _ => Vec::new(),
    }
}

pub fn draw(w: &Widget, c: &mut Canvas2D, data: &Value, t: &Theme) {
    let r = widget_rect(w);
    match w.kind.as_str() {
        "clock_dial" => clock_dial::draw(c, r, &w.config, data, t),
        "ha" => ha::draw(c, r, &w.config, data, t),
        _ => placeholder::draw(c, &w.id, &w.kind, r, t),
    }
}
