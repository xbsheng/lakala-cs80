use chrono::Local;
use serde_json::Value;
use skia_safe::Rect;

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::pages::RustPage;
use crate::theme::Theme;
use crate::widgets::common::{card, fresh_stamp_stock, minute_chart, minute_slots, parse_points, stock_poll_ttl_or, stock_polling, stock_row};

pub struct StockPage;
pub static STOCK: StockPage = StockPage;

impl RustPage for StockPage {
    fn needs(&self, cfg: &Value) -> Vec<Need> {
        let symbols = cfg.get("symbols").and_then(Value::as_str).unwrap_or("sh000001");
        vec![Need::new("portfolio", vec![("symbols".into(), symbols.into())], stock_poll_ttl_or(20))]
    }

    fn draw(&self, c: &mut Canvas2D, size: (f32, f32), data: &Value, cfg: &Value, t: &Theme) {
        let (w, h) = size;
        c.set_fill(t.bg);
        c.fill_rect(Rect::from_xywh(0.0, 0.0, w, h));
        let pad = 36.0;
        c.set_baseline(Baseline::Top);
        c.set_align(Align::Left);
        c.set_fill(t.fg4);
        c.set_font(20.0, 600, false);
        let title = cfg.get("title").and_then(Value::as_str).unwrap_or("自选股").to_uppercase();
        c.tracked_text(&title, pad, pad, 2.6);
        if let Some((stamp, color)) = fresh_stamp_stock(data, stock_polling()) {
            c.set_font(18.0, 400, true);
            c.set_fill(if color == skia_safe::Color::TRANSPARENT { t.fg4 } else { color });
            c.fill_text(&stamp, pad, pad + 30.0);
        }
        c.set_align(Align::Right);
        c.set_font(18.0, 400, true);
        c.set_fill(t.fg3);
        c.fill_text(&Local::now().format("%H:%M:%S").to_string(), w - pad, pad);

        let Some(items) = data.get("items").and_then(Value::as_array) else {
            c.set_align(Align::Left);
            c.set_fill(t.fg4);
            c.set_font(22.0, 400, false);
            let msg = data.get("error").and_then(Value::as_str).map(|e| format!("数据源报错：{e}")).unwrap_or_else(|| "加载中…".into());
            c.fill_text(&msg, pad, pad + 70.0);
            return;
        };
        if items.is_empty() {
            c.set_align(Align::Left);
            c.set_fill(t.fg4);
            c.set_font(22.0, 400, false);
            c.fill_text("加载中…", pad, pad + 70.0);
            return;
        }
        let lead = &items[0];
        let card_y = pad + 56.0;
        let card_h = 420.0;
        c.set_fill(t.card);
        c.fill_rrect(Rect::from_xywh(pad, card_y, w - pad * 2.0, card_h), 12.0);
        c.set_stroke(skia_safe::Color::from_argb(0x0f, 0xff, 0xff, 0xff));
        c.set_line_width(1.0);
        c.stroke_rrect(Rect::from_xywh(pad, card_y, w - pad * 2.0, card_h), 12.0);

        let pct = lead.get("pct").and_then(Value::as_f64).unwrap_or(0.0);
        let up = pct >= 0.0;
        let color = if up { t.up } else { t.down };
        let name = lead.get("name").and_then(Value::as_str)
            .or_else(|| lead.get("symbol").and_then(Value::as_str)).unwrap_or("");
        c.set_align(Align::Left);
        c.set_fill(t.fg3);
        c.set_font(24.0, 500, false);
        let name = c.ellipsis(name, w * 0.5);
        c.fill_text(&name, pad + 28.0, card_y + 26.0);
        let price = lead.get("price").and_then(Value::as_f64).unwrap_or(0.0);
        let price_txt = format!("{price:.2}");
        let pct_txt = format!("{}{:.2}%", if up { "+" } else { "" }, pct);
        c.set_fill(color);
        c.set_font(72.0, 300, true);
        let price_w = c.measure_text(&price_txt);
        c.fill_text(&price_txt, pad + 28.0, card_y + 66.0);
        c.set_align(Align::Right);
        c.set_font(30.0, 500, true);
        c.fill_text(&pct_txt, w - pad - 28.0, card_y + 82.0);
        c.set_align(Align::Left);
        c.set_fill(t.fg4);
        c.set_font(19.0, 400, true);
        let pre = lead.get("preClose").and_then(Value::as_f64)
            .or_else(|| lead.get("prev").and_then(Value::as_f64)).unwrap_or(0.0);
        c.fill_text(&format!("昨收 {pre:.2}"), pad + 28.0, card_y + 158.0);
        let points = parse_points(lead.get("points").unwrap_or(&Value::Null));
        let chart = Rect::from_xywh(pad + 28.0, card_y + 186.0, w - pad * 2.0 - 56.0, card_h - 202.0);
        minute_chart(
            c,
            chart,
            &points,
            pre,
            color,
            minute_slots(lead.get("symbol").and_then(Value::as_str).unwrap_or("")),
        );

        let rest = &items[1..];
        let mut y = card_y + card_h + 30.0;
        let row_h = if rest.is_empty() { 92.0 } else { ((h - y - 96.0) / rest.len() as f32).clamp(78.0, 124.0) };
        for it in rest.iter().take(7) {
            stock_row(c, Rect::from_xywh(pad, y, w - pad * 2.0, row_h - 12.0), it, t, true, None);
            y += row_h;
        }

        c.set_align(Align::Left);
        c.set_fill(t.fg4);
        c.set_font(16.0, 400, true);
        let source = data.get("source").and_then(Value::as_str).unwrap_or("—");
        c.fill_text(&format!("data: {source} · {} symbols", items.len()), pad, h - 48.0);
        if let Some(src) = items[0].get("source").and_then(Value::as_str) {
            c.set_align(Align::Right);
            c.fill_text(&format!("quote: {src}"), w - pad, h - 48.0);
        }
        let _ = price_w;
        let _ = card;
    }
}
