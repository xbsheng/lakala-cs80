use std::time::Duration;

use serde_json::Value;
use skia_safe::Rect;

use crate::assets::Assets;
use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{bar, card};

pub fn needs(_cfg: &Value) -> Vec<Need> {
    vec![Need::new("system", vec![], Duration::from_secs(1))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme, _assets: &mut Assets) {
    let pad = (r.width() * 0.035).max(10.0);
    let cpu = d.get("cpu").and_then(Value::as_f64).unwrap_or(0.0);
    let mem = d.get("mem").and_then(Value::as_f64).unwrap_or(0.0);
    let total = d.get("memTotal").and_then(Value::as_f64).unwrap_or(0.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("电脑");
    let host = d.get("host").and_then(Value::as_str).unwrap_or("");
    let inner = card(c, r, t, title, host, t.fg4, pad);
    let rows = [
        ("CPU", format!("{}%", (cpu * 100.0).round() as i64), cpu, if cpu > 0.85 { t.up } else { t.accent }),
        (
            "MEM",
            if total > 0.0 { format!("{mem:.1}G / {total:.0}G") } else { format!("{mem:.1}G") },
            if total > 0.0 { mem / total } else { 0.0 },
            t.fg3,
        ),
    ];
    let row_h = inner.rect.height() / rows.len() as f32;
    let label_size = (inner.rect.height() * 0.14).clamp(11.0, 18.0);
    for (i, (name, val, frac, color)) in rows.iter().enumerate() {
        let y = inner.rect.top + i as f32 * row_h;
        c.set_baseline(Baseline::Top);
        c.set_align(Align::Left);
        c.set_font(label_size, 500, false);
        c.set_fill(t.fg4);
        c.fill_text(name, inner.rect.left, y);
        c.set_align(Align::Right);
        c.set_fill(t.fg2);
        c.set_font(label_size, 400, true);
        c.fill_text(val, inner.rect.right, y);
        let bar_h = (row_h * 0.16).clamp(4.0, 8.0);
        bar(c, inner.rect.left, y + label_size + row_h * 0.2, inner.rect.width(), bar_h, *frac as f32, *color, t);
    }
}
