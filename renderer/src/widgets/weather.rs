use std::time::Duration;

use serde_json::Value;
use skia_safe::Rect;

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::card;

pub fn needs(cfg: &Value) -> Vec<Need> {
    let city = cfg.get("city").and_then(Value::as_str).unwrap_or("北京");
    vec![Need::new("weather", vec![("city".into(), city.into())], Duration::from_secs(600))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let pad = (r.width() * 0.06).max(10.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("天气");
    let humidity = d.get("humidity").and_then(Value::as_f64);
    let meta = humidity
        .map(|v| format!("湿度 {}%", v.round() as i64))
        .or_else(|| d.get("city").and_then(Value::as_str).map(str::to_string))
        .or_else(|| cfg.get("city").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let inner = card(c, r, t, title, &meta, t.fg4, pad);
    let temp = d.get("temp").and_then(Value::as_f64);
    if temp.is_none() {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.16).max(13.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.fill_text(
            if d.get("error").is_some() { "暂无数据" } else { "加载中…" },
            inner.rect.left,
            inner.rect.top,
        );
        return;
    }
    let temp = temp.unwrap();
    c.set_baseline(Baseline::Top);
    c.set_align(Align::Left);
    c.set_fill(t.fg);
    let size = (inner.rect.height() * 0.5).min(inner.rect.width() * 0.42);
    c.set_font(size, 300, false);
    c.fill_text(
        &format!("{}°", temp.round() as i64),
        inner.rect.left,
        inner.rect.top + inner.rect.height() * 0.12,
    );
    let low = d.get("low").and_then(Value::as_f64);
    let high = d.get("high").and_then(Value::as_f64);
    let range = [
        low.map(|v| format!("↓{}°", v.round() as i64)),
        high.map(|v| format!("↑{}°", v.round() as i64)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("   ");
    if !range.is_empty() {
        c.set_fill(t.fg3);
        c.set_font((inner.rect.height() * 0.14).max(11.0), 400, false);
        c.fill_text(
            &range,
            inner.rect.left,
            inner.rect.top + inner.rect.height() * 0.74,
        );
    }
}
