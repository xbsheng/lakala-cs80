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
    let pad = (r.width() * 0.05).max(10.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("便签");
    let inner = card(c, r, t, title, "", t.fg4, pad);
    let size = (inner.rect.height() * 0.2).clamp(12.0, 34.0);
    c.set_font(size, 400, false);
    c.set_align(Align::Left);
    c.set_baseline(Baseline::Top);
    c.set_fill(t.fg2);
    let text = cfg.get("text").and_then(Value::as_str).unwrap_or("");
    let mut line = String::new();
    let mut y = inner.rect.top;
    for ch in text.chars() {
        let candidate = format!("{line}{ch}");
        if c.measure_text(&candidate) > inner.rect.width() {
            c.fill_text(&line, inner.rect.left, y);
            line = ch.to_string();
            y += size * 1.35;
        } else {
            line = candidate;
        }
        if y > inner.rect.bottom - size {
            return;
        }
    }
    if !line.is_empty() {
        c.fill_text(&line, inner.rect.left, y);
    }
}
