use std::time::Duration;

use chrono::{Datelike, Local, Timelike};
use serde_json::Value;
use skia_safe::Rect;

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::lunar::solar_to_lunar;
use crate::theme::Theme;
use crate::widgets::common::{card, draw_seven_seg, seven_seg_width};

pub fn needs(cfg: &Value) -> Vec<Need> {
    let city = cfg.get("city").and_then(Value::as_str).unwrap_or("北京");
    vec![Need::new("weather", vec![("city".into(), city.into())], Duration::from_secs(600))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let now = Local::now();
    let pad = (r.width() * 0.034).max(10.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("时钟");
    let inner = card(c, r, t, title, "", t.fg4, pad);

    if cfg.get("showWeather").and_then(Value::as_bool).unwrap_or(true) && inner.rect.width() > 300.0 {
        if let Some(temp) = d.get("temp").and_then(Value::as_f64) {
            let temp_size = (inner.rect.height() * 0.16).clamp(14.0, 34.0);
            c.set_font(temp_size, 400, true);
            c.set_align(Align::Right);
            c.set_baseline(Baseline::Top);
            c.set_fill(t.fg2);
            c.fill_text(&format!("{}°", temp.round() as i64), inner.rect.right, inner.head_y - temp_size * 0.02);
        }
    }

    let time = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    let style = cfg.get("font").and_then(Value::as_str).unwrap_or("mono");
    if style == "lcd" {
        let max_w = inner.rect.width() * 0.99;
        let max_h = (inner.rect.height() * 0.62).min(inner.rect.width() * 0.22);
        let mut h = max_h;
        while h > 12.0 && seven_seg_width(&time, h) > max_w {
            h *= 0.94;
        }
        let w_seg = seven_seg_width(&time, h);
        draw_seven_seg(
            c,
            &time,
            inner.rect.left + (inner.rect.width() - w_seg) / 2.0,
            inner.rect.top + inner.rect.height() * 0.5 - h * 0.5,
            h,
            t.fg,
            skia_safe::Color::from_argb(0x0d, 0xff, 0xff, 0xff),
        );
    } else {
        let size = c.fit_font(
            &time,
            inner.rect.width() * 0.99,
            (inner.rect.height() * 0.52).min(inner.rect.width() * 0.2),
            300,
            style == "mono",
        );
        c.set_font(size, 300, style == "mono");
        c.set_fill(t.fg);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Middle);
        c.fill_text(&time, inner.rect.left, inner.rect.top + inner.rect.height() * 0.5);
    }

    let date_size = (inner.rect.height() * 0.115).clamp(11.0, 22.0);
    let date_txt = format!("{}年{}月{}日", now.year(), now.month(), now.day());
    let weekday = format!("星期{}", ["日", "一", "二", "三", "四", "五", "六"][now.weekday().num_days_from_sunday() as usize]);
    let lunar = solar_to_lunar(now.year(), now.month() as i32, now.day() as i32);
    let lunar = format!("农历{lunar}");
    let gap = date_size * 1.1;
    let total = c.measure_text(&date_txt) + c.measure_text(&weekday) + gap
        + if lunar.is_empty() { 0.0 } else { c.measure_text(&lunar) + gap };
    let size2 = if total > inner.rect.width() {
        c.fit_font(&format!("{date_txt}  {weekday}  {lunar}"), inner.rect.width(), date_size, 400, false)
    } else {
        date_size
    };
    c.set_font(size2, 400, false);
    c.set_align(Align::Left);
    c.set_baseline(Baseline::Bottom);
    c.set_fill(t.fg3);
    let base_y = inner.rect.bottom;
    let mut x = inner.rect.left;
    c.fill_text(&date_txt, x, base_y);
    x += c.measure_text(&date_txt) + gap;
    c.fill_text(&weekday, x, base_y);
    x += c.measure_text(&weekday) + gap;
    if !lunar.is_empty() {
        c.fill_text(&lunar, x, base_y);
    }
}
