use serde_json::Value;
use skia_safe::Rect;

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{card, fresh_stamp, stock_poll_ttl, stock_row, stock_session, stock_symbols};

pub fn needs(cfg: &Value) -> Vec<Need> {
    vec![Need::new(
        "portfolio",
        vec![("symbols".into(), stock_symbols(cfg))],
        stock_poll_ttl(),
    )]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let pad = (r.width() * 0.025).max(8.0);
    let session = stock_session();
    let title = cfg.get("title").and_then(Value::as_str)
        .unwrap_or(if cfg.get("us").is_some() || cfg.get("usFut").is_some() { session.label } else { "股票" });
    let stamp = fresh_stamp(d);
    let meta = stamp.as_ref().map(|(s, _)| s.as_str()).unwrap_or("");
    let meta_color = stamp.as_ref().map(|(_, c)| *c).unwrap_or(t.fg4);
    let inner = card(c, r, t, title, meta, meta_color, pad);
    let Some(items) = d.get("items").and_then(Value::as_array) else {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.14).max(12.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.fill_text(if d.get("error").is_some() { "暂无数据" } else { "加载中…" }, inner.rect.left, inner.rect.top);
        return;
    };
    if items.is_empty() {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.14).max(12.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.fill_text("加载中…", inner.rect.left, inner.rect.top);
        return;
    }
    let gap = (inner.rect.height() * 0.02).max(4.0);
    let fit = ((inner.rect.height() + gap) / (96.0 + gap)).floor() as usize;
    let per_cell = ((r.height() / 160.0).round() as usize).max(1);
    let rows = items.len().min(fit.max(1)).min(per_cell).max(1);
    let row_h = (inner.rect.height() - gap * (rows as f32 - 1.0)) / rows as f32;
    let row_h = row_h.min(200.0);
    let slots = match session.id { "cn" => 241.0, "us" => 391.0, _ => 1380.0 };
    for (i, item) in items.iter().take(rows).enumerate() {
        stock_row(
            c,
            Rect::from_xywh(inner.rect.left, inner.rect.top + i as f32 * (row_h + gap), inner.rect.width(), row_h),
            item,
            t,
            false,
            Some(slots),
        );
    }
}
