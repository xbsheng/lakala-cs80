use std::time::Duration;

use chrono::{Datelike, Local, Timelike};
use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::card;

pub fn needs(cfg: &Value) -> Vec<Need> {
    let city = cfg
        .get("city")
        .and_then(Value::as_str)
        .unwrap_or("北京")
        .to_string();
    vec![Need::new(
        "weather",
        vec![("city".to_string(), city)],
        Duration::from_secs(600),
    )]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, data: &Value, t: &Theme) {
    let now = Local::now();
    let pad = (r.width() * 0.02).max(8.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("");
    let show_title = !title.is_empty();
    let inner = card(c, r, t, title, "", t.fg4, pad);
    let _ = inner;

    let box_ = Rect::from_xywh(r.left + 3.0, r.top + 3.0, r.width() - 6.0, r.height() - 6.0);
    let cx = box_.left + box_.width() / 2.0;
    let cy = box_.top + box_.height() * if show_title { 0.56 } else { 0.52 };

    // 刻度：每 6° 一根，沿圆心到圆角矩形边界的射线落点。
    let inset = box_.width().min(box_.height()) * 0.028;
    let tick_base = box_.width().min(box_.height());
    let gw = box_.width() - inset * 2.0;
    let gh = box_.height() - inset * 2.0;
    let corner_r = (box_.height() * 0.1).min(gw / 2.0).min(gh / 2.0);
    let ox = cx - (box_.left + box_.width() / 2.0);
    let oy = cy - (box_.top + box_.height() / 2.0);
    c.save();
    c.set_line_cap_round();
    for i in 0..60 {
        let major = i % 5 == 0;
        let ang = (i as f32 / 60.0) * std::f32::consts::PI * 2.0 - std::f32::consts::PI / 2.0;
        let edge = rr_edge_distance(ox, oy, ang, gw, gh, corner_r);
        let ux = ang.cos();
        let uy = ang.sin();
        let px = cx + ux * edge;
        let py = cy + uy * edge;
        let len = tick_base * if major { 0.075 } else { 0.04 };
        c.set_stroke(if major {
            Color::from_argb(0xcc, 0xff, 0xff, 0xff)
        } else {
            Color::from_argb(0x42, 0xff, 0xff, 0xff)
        });
        c.set_line_width((tick_base * if major { 0.01 } else { 0.0045 }).max(1.2));
        c.line(px, py, px - ux * len, py - uy * len);
    }
    c.restore();

    let hh = format!("{:02}", now.hour());
    let mm = format!("{:02}", now.minute());
    let num_size = (box_.height() * 0.36).min(box_.width() * 0.19);
    let hand_gap = num_size * 0.62;

    // 日期 + 星期
    let date_size = (box_.height() * 0.11).clamp(11.0, 26.0);
    let date_txt = format!("{}月 {}日", now.month(), now.day());
    let weekday = format!(
        "周{}",
        ["日", "一", "二", "三", "四", "五", "六"][now.weekday().num_days_from_sunday() as usize]
    );
    c.set_font(date_size, 500, false);
    c.set_baseline(Baseline::Middle);
    let d_gap = date_size * 0.9;
    let date_w = c.measure_text(&date_txt);
    let week_w = c.measure_text(&weekday);
    let d_w = date_w + week_w + d_gap;
    let d_y = box_.top + box_.height() * if show_title { 0.24 } else { 0.2 };
    c.set_align(Align::Left);
    c.set_fill(t.fg2);
    c.fill_text(&date_txt, cx - d_w / 2.0, d_y);
    c.set_fill(t.accent);
    c.fill_text(&weekday, cx - d_w / 2.0 + date_w + d_gap, d_y);

    // 指针
    let min_len = (box_.width() * 0.26).min(box_.height() * 0.33);
    let hour_len = min_len * 0.58;
    let to_rad = |deg: f32| (deg - 90.0) * std::f32::consts::PI / 180.0;
    let hour_ang = to_rad(((now.hour() % 12) as f32 + now.minute() as f32 / 60.0) * 30.0);
    let min_ang = to_rad(now.minute() as f32 * 6.0);
    // 秒针用亚秒角度：5 Hz 渲染时是连续走，而不是每秒跳一下。
    let sec_ang = to_rad((now.second() as f32 + now.nanosecond() as f32 / 1_000_000_000.0) * 6.0);
    let thick = (num_size * 0.075).max(2.5);
    c.set_stroke(t.fg);
    c.set_line_cap_round();
    c.set_line_width(thick);
    for (a, len) in [(hour_ang, hour_len), (min_ang, min_len)] {
        c.line(
            cx - a.cos() * len * 0.18,
            cy - a.sin() * len * 0.18,
            cx + a.cos() * len,
            cy + a.sin() * len,
        );
    }
    c.set_stroke(t.accent);
    c.set_line_width((thick * 0.32).max(1.4));
    let sec_len = (box_.width() * 0.42).min(box_.height() * 0.42);
    c.line(
        cx - sec_ang.cos() * sec_len * 0.18,
        cy - sec_ang.sin() * sec_len * 0.18,
        cx + sec_ang.cos() * sec_len,
        cy + sec_ang.sin() * sec_len,
    );

    // 数字
    c.set_font(num_size, 700, false);
    c.set_fill(t.fg);
    c.set_baseline(Baseline::Middle);
    c.set_align(Align::Right);
    c.fill_text(&hh, cx - hand_gap, cy);
    c.set_align(Align::Left);
    c.fill_text(&mm, cx + hand_gap, cy);

    // 中心轴
    c.set_fill(t.card);
    c.fill_circle(cx, cy, thick * 1.35);
    c.set_stroke(t.accent);
    c.set_line_width((thick * 0.45).max(1.6));
    c.stroke_circle(cx, cy, thick * 1.05);
    c.set_fill(t.accent);
    c.fill_circle(cx, cy, thick * 0.45);

    // 天气
    let show_weather = cfg
        .get("showWeather")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let temp = data.get("temp").and_then(Value::as_f64);
    if show_weather {
        if let Some(temp) = temp {
            let code = data.get("code").and_then(Value::as_i64).unwrap_or(3);
            let icon_size = (box_.height() * 0.2).min(46.0);
            let txt_size = (box_.height() * 0.14).clamp(11.0, 26.0);
            let label = format!("{} {}°", weather_text(code), temp.round() as i64);
            c.set_font(txt_size, 500, false);
            let tw = c.measure_text(&label);
            let gap = icon_size * 0.35;
            let total_w = icon_size + gap + tw;
            let bx = cx - total_w / 2.0;
            let by = box_.top + box_.height() * 0.86;
            weather_icon(c, bx + icon_size / 2.0, by, icon_size, code);
            c.set_fill(t.fg2);
            c.set_align(Align::Left);
            c.set_baseline(Baseline::Middle);
            c.fill_text(&label, bx + icon_size + gap, by);
        }
    }
}

fn rr_edge_distance(ox: f32, oy: f32, ang: f32, w: f32, h: f32, r: f32) -> f32 {
    let hw = w / 2.0;
    let hh = h / 2.0;
    let cos = ang.cos();
    let sin = ang.sin();
    let inside = |d: f32| {
        let x = (ox + cos * d).abs();
        let y = (oy + sin * d).abs();
        if x > hw || y > hh {
            return false;
        }
        let dx = (x - (hw - r)).max(0.0);
        let dy = (y - (hh - r)).max(0.0);
        dx * dx + dy * dy <= r * r
    };
    let mut lo = 0.0;
    let mut hi = (w * w + h * h).sqrt();
    for _ in 0..28 {
        let mid = (lo + hi) / 2.0;
        if inside(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn weather_kind(code: i64) -> &'static str {
    match code {
        0 => "clear",
        1 | 2 => "partly",
        3 => "cloud",
        45 | 48 => "fog",
        51..=57 => "drizzle",
        61..=67 | 80..=82 => "rain",
        71..=77 | 85 | 86 => "snow",
        95..=99 => "thunder",
        _ => "cloud",
    }
}

fn weather_text(code: i64) -> &'static str {
    match code {
        0 => "晴",
        1 | 2 => "多云",
        3 => "阴",
        45 | 48 => "有雾",
        51..=57 => "毛毛雨",
        61..=65 => "雨",
        66 | 67 => "冻雨",
        71..=77 | 85 | 86 => "雪",
        80..=82 => "阵雨",
        95..=99 => "雷暴",
        _ => "多云",
    }
}

const WX_SUN: Color = Color::from_rgb(0xe8, 0xc1, 0x69);
const WX_CLOUD: Color = Color::from_rgb(0x9a, 0xa3, 0xb2);
const WX_RAIN: Color = Color::from_rgb(0x5a, 0xa9, 0xff);
const WX_SNOW: Color = Color::from_rgb(0xd6, 0xd7, 0xdc);
const WX_BOLT: Color = Color::from_rgb(0xfa, 0xbe, 0x46);

fn weather_icon(c: &mut Canvas2D, cx: f32, cy: f32, size: f32, code: i64) {
    let kind = weather_kind(code);
    let s = size / 100.0;
    c.save();
    c.translate(cx, cy);
    c.scale(s, s);
    c.set_line_cap_round();
    c.set_line_join_round();

    match kind {
        "clear" => sun(c, 0.0, 0.0, 26.0),
        "partly" => {
            sun(c, -18.0, -20.0, 20.0);
            cloud(c, 4.0, 6.0, 0.92);
        }
        "cloud" => cloud(c, 0.0, -4.0, 1.0),
        "fog" => {
            cloud(c, 0.0, -12.0, 0.95);
            c.set_stroke(WX_CLOUD);
            c.set_line_width(5.0);
            for (i, y) in [-2.0f32, 12.0, 26.0].iter().enumerate() {
                c.line(-26.0 + i as f32 * 4.0, y + 26.0, 26.0 - i as f32 * 4.0, y + 26.0);
            }
        }
        "drizzle" => {
            cloud(c, 0.0, -8.0, 0.95);
            drops(c, 3, 10.0);
        }
        "rain" => {
            cloud(c, 0.0, -10.0, 0.95);
            drops(c, 3, 18.0);
        }
        "snow" => {
            cloud(c, 0.0, -10.0, 0.95);
            flakes(c, 3);
        }
        "thunder" => {
            cloud(c, 0.0, -10.0, 0.95);
            c.set_fill(WX_BOLT);
            c.fill_polygon(&[
                (2.0, 26.0),
                (-10.0, 48.0),
                (2.0, 48.0),
                (-4.0, 66.0),
                (14.0, 40.0),
                (2.0, 40.0),
                (10.0, 26.0),
            ]);
        }
        _ => {}
    }
    c.restore();
}

fn sun(c: &mut Canvas2D, x: f32, y: f32, r: f32) {
    c.set_fill(WX_SUN);
    c.fill_circle(x, y, r);
    c.set_stroke(WX_SUN);
    c.set_line_width(6.0);
    for i in 0..8 {
        let a = i as f32 / 8.0 * std::f32::consts::PI * 2.0;
        c.line(
            x + a.cos() * (r + 8.0),
            y + a.sin() * (r + 8.0),
            x + a.cos() * (r + 18.0),
            y + a.sin() * (r + 18.0),
        );
    }
}

fn cloud(c: &mut Canvas2D, x: f32, y: f32, scale: f32) {
    c.save();
    c.translate(x, y);
    c.scale(scale, scale);
    c.set_fill(WX_CLOUD);
    c.fill_circle(-16.0, 4.0, 17.0);
    c.fill_circle(4.0, -6.0, 22.0);
    c.fill_circle(24.0, 5.0, 16.0);
    c.fill_rrect(Rect::from_xywh(-32.0, 4.0, 64.0, 22.0), 11.0);
    c.restore();
}

fn drops(c: &mut Canvas2D, n: usize, len: f32) {
    c.set_stroke(WX_RAIN);
    c.set_line_width(6.0);
    for i in 0..n {
        let x = -20.0 + i as f32 * 18.0;
        c.line(x, 32.0, x - 6.0, 32.0 + len);
    }
}

fn flakes(c: &mut Canvas2D, n: usize) {
    c.set_stroke(WX_SNOW);
    c.set_line_width(4.0);
    for i in 0..n {
        let x = -20.0 + i as f32 * 18.0;
        let y = 40.0;
        c.line(x - 6.0, y - 6.0, x + 6.0, y + 6.0);
        c.line(x + 6.0, y - 6.0, x - 6.0, y + 6.0);
    }
}
