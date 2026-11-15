use chrono::{Datelike, Local, Timelike};
use serde_json::Value;
use skia_safe::Rect;

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::card;

pub fn needs(_cfg: &Value) -> Vec<Need> {
    Vec::new()
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, _d: &Value, t: &Theme) {
    let pad = (r.width() * 0.06).max(10.0);
    let label = cfg.get("label").and_then(Value::as_str).unwrap_or("倒计时");
    let time = cfg.get("time").and_then(Value::as_str).unwrap_or("18:00");
    let inner = card(c, r, t, label, time, t.fg4, pad);
    let now = Local::now();
    let (h, m) = time
        .split_once(':')
        .and_then(|(h, m)| Some((h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)))
        .unwrap_or((18, 0));
    let target = now
        .with_hour(h)
        .and_then(|d| d.with_minute(m))
        .and_then(|d| d.with_second(0))
        .and_then(|d| d.with_nanosecond(0))
        .unwrap_or(now);
    let target = if target <= now { target + chrono::Duration::days(1) } else { target };
    let left = (target - now).num_seconds().max(0);
    let txt = format!("{:02}:{:02}:{:02}", left / 3600, (left / 60) % 60, left % 60);
    let size = c.fit_font(&txt, inner.rect.width(), (inner.rect.height() * 0.5).min(inner.rect.width() * 0.34), 300, true);
    c.set_font(size, 300, true);
    c.set_fill(t.fg);
    c.set_align(Align::Left);
    c.set_baseline(Baseline::Top);
    c.fill_text(&txt, inner.rect.left, inner.rect.top + inner.rect.height() * 0.2);
    let _ = now.day();
}
