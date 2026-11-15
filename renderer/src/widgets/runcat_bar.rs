use std::time::Duration;

use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::assets::Assets;
use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{card, net_text, sparkline};

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
    let pad = (r.width() * 0.018).max(12.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("RunCat");
    let host = d.get("host").and_then(Value::as_str).unwrap_or("");
    let inner = card(c, r, t, title, host, t.fg4, pad);
    assets.sample(cpu, mem);

    let lane_w = (inner.rect.width() * 0.38).min(inner.rect.height() * 1.1);
    let base_y = inner.rect.top + inner.rect.height() * 0.6;
    let cat_size = (lane_w * 0.98).min((base_y - inner.rect.top) / 0.78);
    c.set_stroke(Color::from_argb(0x14, 0xff, 0xff, 0xff));
    c.set_line_width(1.0);
    c.line(inner.rect.left, base_y.round() + 0.5, inner.rect.left + lane_w, base_y.round() + 0.5);
    let frame = assets.cat_frame(cpu);
    assets.draw_cat(c, inner.rect.left + lane_w / 2.0, base_y, cat_size, t.fg, frame, true);
    c.set_align(Align::Center);
    c.set_baseline(Baseline::Top);
    c.set_font((lane_w * 0.07).clamp(11.0, 18.0), 400, true);
    c.set_fill(t.fg3);
    c.fill_text(&net_text(d), inner.rect.left + lane_w / 2.0, base_y + (inner.rect.height() * 0.055).max(10.0));

    let x0 = inner.rect.left + lane_w + (inner.rect.width() * 0.028).max(18.0);
    let col_w = inner.rect.right - x0;
    let num_w = col_w * 0.42;
    let row_h = inner.rect.height() / 2.0;
    let rows = vec![
        (
            "CPU".to_string(),
            format!("{}%", (cpu * 100.0).round() as i64),
            assets.history.cpu.clone(),
            if cpu > 0.85 { t.up } else { RUNCAT_CPU },
        ),
        (
            format!("MEM{}", if mem_total > 0.0 { format!(" {}G", mem_total.round() as i64) } else { String::new() }),
            d.get("mem").and_then(Value::as_f64).map(|v| format!("{v:.1}G")).unwrap_or_else(|| "--".into()),
            assets.history.mem.clone(),
            RUNCAT_MEM,
        ),
    ];
    for (i, (name, val, series, color)) in rows.iter().enumerate() {
        let y = inner.rect.top + i as f32 * row_h;
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.set_font((row_h * 0.12).clamp(11.0, 20.0), 500, false);
        c.set_fill(t.fg4);
        c.fill_text(name, x0, y + row_h * 0.18);
        c.set_fill(t.fg);
        let size = c.fit_font(val, num_w, (row_h * 0.42).min(84.0), 300, true);
        c.set_font(size, 300, true);
        c.fill_text(val, x0, y + row_h * 0.4);
        let sx = x0 + num_w + (col_w * 0.03).max(12.0);
        let sh = (row_h * 0.44).min(60.0);
        sparkline(c, sx, y + row_h * 0.3, inner.rect.right - sx, sh, series, *color);
    }
}
