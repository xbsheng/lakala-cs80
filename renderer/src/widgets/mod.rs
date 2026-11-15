pub mod agent;
pub mod clock;
pub mod clock_dial;
pub mod common;
pub mod crypto;
pub mod github;
pub mod ha;
pub mod holiday;
pub mod kline;
pub mod music;
pub mod news;
pub mod note;
pub mod placeholder;
pub mod runcat;
pub mod runcat_bar;
pub mod stock;
pub mod system;
pub mod timer;
pub mod weather;

use serde_json::Value;
use skia_safe::Rect;

use crate::assets::Assets;
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
        "clock" => clock::needs(&w.config),
        "clock_dial" => clock_dial::needs(&w.config),
        "weather" => weather::needs(&w.config),
        "crypto" => crypto::needs(&w.config),
        "github" => github::needs(&w.config),
        "music" => music::needs(&w.config),
        "timer" => timer::needs(&w.config),
        "note" => note::needs(&w.config),
        "holiday" => holiday::needs(&w.config),
        "ha" => ha::needs(&w.config),
        "agent" => agent::needs(&w.config),
        "system" => system::needs(&w.config),
        "stock" => stock::needs(&w.config),
        "kline" => kline::needs(&w.config),
        "news" => news::needs(&w.config),
        "runcat" => runcat::needs(&w.config),
        "runcat_bar" => runcat_bar::needs(&w.config),
        _ => Vec::new(),
    }
}

pub fn draw(w: &Widget, c: &mut Canvas2D, data: &Value, t: &Theme, assets: &mut Assets) {
    let r = widget_rect(w);
    match w.kind.as_str() {
        "clock" => clock::draw(c, r, &w.config, data, t),
        "clock_dial" => clock_dial::draw(c, r, &w.config, data, t),
        "weather" => weather::draw(c, r, &w.config, data, t),
        "crypto" => crypto::draw(c, r, &w.config, data, t),
        "github" => github::draw(c, r, &w.config, data, t),
        "music" => music::draw(c, r, &w.config, data, t),
        "timer" => timer::draw(c, r, &w.config, data, t),
        "note" => note::draw(c, r, &w.config, data, t),
        "holiday" => holiday::draw(c, r, &w.config, data, t),
        "ha" => ha::draw(c, r, &w.config, data, t),
        "agent" => agent::draw(c, r, &w.config, data, t, assets),
        "system" => system::draw(c, r, &w.config, data, t, assets),
        "stock" => stock::draw(c, r, &w.config, data, t),
        "kline" => kline::draw(c, r, &w.config, data, t),
        "news" => news::draw(c, r, &w.config, data, t),
        "runcat" => runcat::draw(c, r, &w.config, data, t, assets),
        "runcat_bar" => runcat_bar::draw(c, r, &w.config, data, t, assets),
        _ => placeholder::draw(c, &w.id, &w.kind, r, t),
    }
}
