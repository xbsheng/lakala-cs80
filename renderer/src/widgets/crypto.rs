use std::time::Duration;

use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{card, pct_color, rows_for, signed};

pub fn needs(cfg: &Value) -> Vec<Need> {
    let ids = cfg.get("ids").and_then(Value::as_str).unwrap_or("bitcoin,ethereum");
    vec![Need::new("crypto", vec![("ids".into(), ids.into())], Duration::from_secs(60))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let pad = (r.width() * 0.06).max(10.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("加密");
    let inner = card(c, r, t, title, "", t.fg4, pad);
    let Some(coins) = d.get("coins").and_then(Value::as_array) else {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.16).max(13.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.fill_text(if d.get("error").is_some() { "暂无数据" } else { "加载中…" }, inner.rect.left, inner.rect.top);
        return;
    };
    let (rows, row_h) = rows_for(inner.rect.height(), (inner.rect.width() * 0.055).clamp(34.0, 56.0), coins.len());
    for (i, coin) in coins.iter().take(rows).enumerate() {
        let yy = inner.rect.top + i as f32 * row_h + row_h * 0.12;
        let price = coin.get("price").and_then(Value::as_f64).unwrap_or(0.0);
        let pct = coin.get("pct").and_then(Value::as_f64).unwrap_or(0.0);
        c.set_baseline(Baseline::Top);
        c.set_align(Align::Left);
        c.set_font((row_h * 0.32).clamp(11.0, 17.0), 500, true);
        c.set_fill(t.fg3);
        c.fill_text(coin.get("symbol").and_then(Value::as_str).unwrap_or(""), inner.rect.left, yy);
        c.set_align(Align::Right);
        c.set_fill(t.fg2);
        c.set_font((row_h * 0.32).clamp(11.0, 17.0), 400, true);
        let price_txt = if price >= 1000.0 {
            format!("${price:.0}")
        } else if price < 10.0 {
            format!("${price:.3}")
        } else {
            format!("${price:.2}")
        };
        c.fill_text(&price_txt, inner.rect.right, yy);
        c.set_fill(pct_color(pct, t));
        c.set_font((row_h * 0.26).clamp(10.0, 14.0), 500, true);
        c.fill_text(&signed(pct, 2), inner.rect.right, yy + row_h * 0.44);
    }
    let _ = Color::TRANSPARENT;
}
