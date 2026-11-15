use std::time::Duration;

use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::assets::Assets;
use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{card, fmt_rate, gauge};

const RUNCAT_CPU: Color = Color::from_rgb(0x5f, 0xc9, 0x8d);
const RUNCAT_MEM: Color = Color::from_rgb(0x5a, 0xa9, 0xff);

pub fn needs(_cfg: &Value) -> Vec<Need> {
    vec![Need::new("system", vec![], Duration::from_secs(1))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme, assets: &mut Assets) {
    let cpu = d.get("cpu").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0) as f32;
    let mem_total = d.get("memTotal").and_then(Value::as_f64).unwrap_or(0.0);
    let mem = if mem_total > 0.0 {
        (d.get("mem").and_then(Value::as_f64).unwrap_or(0.0) / mem_total).clamp(0.0, 1.0) as f32
    } else { 0.0 };
    let pad = (r.width() * 0.04).max(10.0);
    let meta = if cfg.get("meta").and_then(Value::as_str) == Some("host") {
        d.get("host").and_then(Value::as_str).unwrap_or("")
    } else {
        d.get("ip").and_then(Value::as_str)
            .or_else(|| d.get("host").and_then(Value::as_str))
            .unwrap_or("")
    };
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("RunCat");
    let inner = card(c, r, t, title, meta, t.fg4, pad);
    let text_band = (inner.rect.height() * 0.26).max(44.0);
    let r_out = (inner.rect.width() * 0.42).min((inner.rect.height() - text_band) * 0.47);
    let lw = (r_out * 0.14).max(9.0);
    let r_in = r_out - lw * 2.0;
    let cx = inner.rect.left + inner.rect.width() / 2.0;
    let cy = inner.rect.top + r_out + lw / 2.0 + 2.0;
    gauge(c, cx, cy, r_out, lw, cpu, if cpu > 0.85 { t.up } else { RUNCAT_CPU });
    gauge(c, cx, cy, r_in, lw, mem, RUNCAT_MEM);
    let frame = assets.cat_frame(cpu);
    assets.draw_cat(c, cx, cy, r_in * 1.72, t.fg, frame, false);
    let big = (inner.rect.width() * 0.058).clamp(12.0, 21.0);
    let small = (big * 0.78).max(10.0);
    let col_gap = big * 2.2;
    let y_top = inner.rect.bottom - text_band * 0.66;
    let y_bottom = inner.rect.bottom - text_band * 0.16;
    c.set_baseline(Baseline::Middle);
    c.set_align(Align::Right);
    c.set_font(big, 500, true);
    c.set_fill(if cpu > 0.85 { t.up } else { RUNCAT_CPU });
    c.fill_text(&format!("CPU {}%", (cpu * 100.0).round() as i64), cx - col_gap / 2.0, y_top);
    c.set_font(small, 400, true);
    c.set_fill(t.fg3);
    c.fill_text(&format!("↑ {}", fmt_rate(d.get("netUp").and_then(Value::as_f64).unwrap_or(0.0))), cx - col_gap / 2.0, y_bottom);
    c.set_align(Align::Left);
    c.set_font(big, 500, true);
    c.set_fill(RUNCAT_MEM);
    c.fill_text(&format!("MEM {}%", (mem * 100.0).round() as i64), cx + col_gap / 2.0, y_top);
    c.set_font(small, 400, true);
    c.set_fill(t.fg3);
    c.fill_text(&format!("↓ {}", fmt_rate(d.get("netDown").and_then(Value::as_f64).unwrap_or(0.0))), cx + col_gap / 2.0, y_bottom);
}
