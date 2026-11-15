use std::time::Duration;

use chrono::{Datelike, Local, Timelike};
use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{card, pct_color, signed};

pub fn needs(cfg: &Value) -> Vec<Need> {
    let symbol = cfg.get("symbol").and_then(Value::as_str).unwrap_or("sh000001");
    vec![Need::new("kline", vec![("symbol".into(), symbol.into())], Duration::from_secs(120))]
}

fn trading_hours() -> bool {
    let d = Local::now();
    let day = d.weekday().num_days_from_sunday();
    if day == 0 || day == 6 { return false; }
    let m = d.hour() as i32 * 60 + d.minute() as i32;
    (570..=690).contains(&m) || (780..=900).contains(&m)
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let pad = (r.width() * 0.03).max(10.0);
    let title = d.get("name").and_then(Value::as_str)
        .or_else(|| cfg.get("title").and_then(Value::as_str))
        .or_else(|| cfg.get("symbol").and_then(Value::as_str))
        .unwrap_or("K 线");
    let pct = d.get("pct").and_then(Value::as_f64);
    let meta = pct.map(|v| format!("{}%", signed(v, 2))).unwrap_or_default();
    let meta_color = pct.map(|v| pct_color(v, t)).unwrap_or(t.fg4);
    let inner = card(c, r, t, title, &meta, meta_color, pad);
    let Some(candles) = d.get("candles").and_then(Value::as_array) else {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.16).max(13.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.fill_text(if d.get("error").is_some() { "暂无数据" } else { "加载中…" }, inner.rect.left, inner.rect.top);
        return;
    };
    if candles.is_empty() {
        return;
    }
    let candles = &candles[candles.len().saturating_sub(48)..];
    let mut hi = f64::NEG_INFINITY;
    let mut lo = f64::INFINITY;
    for c in candles {
        hi = hi.max(c.get("h").and_then(Value::as_f64).unwrap_or(0.0));
        lo = lo.min(c.get("l").and_then(Value::as_f64).unwrap_or(0.0));
    }
    let span = (hi - lo).max(1e-9);
    let w = inner.rect.width() / candles.len() as f32;
    let y = |v: f64| inner.rect.top + inner.rect.height() - ((v - lo) / span) as f32 * inner.rect.height();
    for (i, cdl) in candles.iter().enumerate() {
        let o = cdl.get("o").and_then(Value::as_f64).unwrap_or(0.0);
        let h = cdl.get("h").and_then(Value::as_f64).unwrap_or(0.0);
        let l = cdl.get("l").and_then(Value::as_f64).unwrap_or(0.0);
        let cl = cdl.get("c").and_then(Value::as_f64).unwrap_or(0.0);
        let x = inner.rect.left + i as f32 * w + w / 2.0;
        let col = if cl >= o { t.up } else { t.down };
        c.set_stroke(col);
        c.set_fill(col);
        c.set_line_width((w * 0.16).max(1.0));
        c.line(x, y(h), x, y(l));
        let bw = (w * 0.55).max(1.5);
        let y1 = y(o.max(cl));
        let y2 = y(o.min(cl));
        c.fill_rect(Rect::from_xywh(x - bw / 2.0, y1, bw, (y2 - y1).max(1.2)));
    }
    let _ = trading_hours();
    let _ = Color::TRANSPARENT;
}
