use skia_safe::{Color, Rect};

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::theme::{Theme, CARD_BORDER};

pub struct CardInner {
    pub rect: Rect,
    pub head_y: f32,
    pub head_h: f32,
    pub head_font: f32,
}

pub fn card(
    c: &mut Canvas2D,
    r: Rect,
    t: &Theme,
    title: &str,
    meta: &str,
    meta_color: Color,
    pad: f32,
) -> CardInner {
    let box_ = Rect::from_xywh(r.left + 3.0, r.top + 3.0, r.width() - 6.0, r.height() - 6.0);
    c.set_fill(t.card);
    c.fill_rrect(box_, 7.0);
    c.set_stroke(CARD_BORDER);
    c.set_line_width(1.0);
    c.stroke_rrect(box_, 7.0);

    let inner = Rect::from_xywh(
        box_.left + pad,
        box_.top + pad,
        box_.width() - pad * 2.0,
        box_.height() - pad * 2.0,
    );
    if title.is_empty() {
        return CardInner {
            rect: inner,
            head_y: box_.top + pad * 0.72,
            head_h: 0.0,
            head_font: 0.0,
        };
    }

    let size = (box_.height() * 0.062).clamp(11.0, 17.0);
    c.set_align(Align::Left);
    c.set_baseline(Baseline::Top);
    c.set_font(size, 600, false);
    c.set_fill(t.fg4);
    c.tracked_text(title, inner.left, box_.top + pad * 0.72, size * 0.1);

    if !meta.is_empty() {
        c.set_align(Align::Right);
        c.set_font(size * 0.92, 500, false);
        c.set_fill(meta_color);
        let text = c.ellipsis(meta, inner.width() * 0.55);
        c.fill_text(&text, inner.right, box_.top + pad * 0.72);
    }

    let head_h = size + pad * 0.9;
    CardInner {
        rect: Rect::from_xywh(inner.left, inner.top + head_h, inner.width(), inner.height() - head_h),
        head_y: box_.top + pad * 0.72,
        head_h,
        head_font: size,
    }
}

pub fn hairline(c: &mut Canvas2D, x1: f32, y1: f32, x2: f32, y2: f32) {
    c.set_stroke(Color::from_argb(0x0f, 0xff, 0xff, 0xff));
    c.set_line_width(1.0);
    c.line(x1, y1, x2, y2);
}

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use chrono::{Datelike, Local, TimeZone, Timelike, Utc};
use serde_json::Value;

use crate::canvas::Baseline as CBaseline;

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn bar(c: &mut Canvas2D, x: f32, y: f32, w: f32, h: f32, frac: f32, color: Color, t: &Theme) {
    c.set_fill(Color::from_argb(0x12, 0xff, 0xff, 0xff));
    c.fill_rrect(Rect::from_xywh(x, y, w, h), h / 2.0);
    let fw = (w * frac.clamp(0.0, 1.0)).max(h);
    c.set_fill(color);
    c.fill_rrect(Rect::from_xywh(x, y, fw, h), h / 2.0);
    let _ = t;
}

pub fn dashed(c: &mut Canvas2D, x1: f32, y1: f32, x2: f32, y2: f32) {
    c.save();
    c.set_dash(&[4.0, 4.0]);
    c.set_stroke(Color::from_argb(0x24, 0xff, 0xff, 0xff));
    c.set_line_width(1.0);
    c.line(x1, y1 + 0.5, x2, y2 + 0.5);
    c.set_dash(&[]);
    c.restore();
}

pub fn rows_for(inner_h: f32, base_row_h: f32, count: usize) -> (usize, f32) {
    let rows = (inner_h / base_row_h).floor().max(1.0) as usize;
    let rows = rows.min(count.max(1));
    (rows, base_row_h.min(inner_h / rows as f32))
}

pub fn pct_color(v: f64, t: &Theme) -> Color {
    if v >= 0.0 { t.up } else { t.down }
}

pub fn signed(v: f64, digits: usize) -> String {
    format!("{}{:.*}", if v >= 0.0 { "+" } else { "" }, digits, v)
}

pub fn big_days(
    c: &mut Canvas2D,
    x: f32,
    y: f32,
    max_w: f32,
    size: f32,
    n: i64,
    t: &Theme,
    align: Align,
    suffix: &str,
) {
    let num = n.to_string();
    let mut s = size;
    let width_of = |c: &mut Canvas2D, s: f32| {
        c.set_font(s, 700, false);
        let nw = c.measure_text(&num);
        if suffix.is_empty() {
            return nw;
        }
        c.set_font(s * 0.42, 500, false);
        nw + s * 0.1 + c.measure_text(suffix)
    };
    while width_of(c, s) > max_w && s > 9.0 {
        s *= 0.93;
    }
    c.set_font(s, 700, false);
    let nw = c.measure_text(&num);
    let gap = if suffix.is_empty() { 0.0 } else { s * 0.1 };
    c.set_font(s * 0.42, 500, false);
    let sw = if suffix.is_empty() { 0.0 } else { c.measure_text(suffix) };
    let total = nw + gap + sw;
    let nx = match align {
        Align::Center => x - total / 2.0,
        Align::Left => x,
        Align::Right => x - total,
    };
    c.set_align(Align::Left);
    c.set_baseline(CBaseline::Alphabetic);
    c.set_font(s, 700, false);
    c.set_fill(t.fg);
    c.fill_text(&num, nx, y);
    if !suffix.is_empty() {
        c.set_font(s * 0.42, 500, false);
        c.set_fill(t.fg3);
        c.fill_text(suffix, nx + nw + gap, y);
    }
}

pub fn fresh_stamp(d: &Value) -> Option<(String, Color)> {
    let ms = d.get("fetchedAt")?.as_i64()?;
    if d.get("stale").and_then(Value::as_bool).unwrap_or(false) {
        return Some(("更新失败·重取中".into(), Color::from_rgb(0xfa, 0xbe, 0x46)));
    }
    let at = Local.timestamp_millis_opt(ms).single()?;
    let today = Local::now().date_naive() == at.date_naive();
    if today {
        Some((format!("{} 更新", at.format("%H:%M")), Color::TRANSPARENT))
    } else {
        Some((format!("{}", at.format("%-m/%-d %H:%M")), Color::TRANSPARENT))
    }
}

pub struct StockSession {
    pub id: &'static str,
    pub label: &'static str,
}

fn in_window(m: i32, a: i32, b: i32) -> bool {
    m >= a && m <= b
}

pub fn stock_session() -> StockSession {
    let cn = Utc::now().with_timezone(&chrono_tz::Asia::Shanghai);
    let cn_day = cn.weekday().num_days_from_sunday() as i32;
    let cn_min = cn.hour() as i32 * 60 + cn.minute() as i32;
    // A 股 9:15 开始集合竞价，15:00 收盘
    if (1..=5).contains(&cn_day) && in_window(cn_min, 555, 900) {
        return StockSession { id: "cn", label: "自选股" };
    }
    let us = Utc::now().with_timezone(&chrono_tz::America::New_York);
    let us_day = us.weekday().num_days_from_sunday() as i32;
    let us_min = us.hour() as i32 * 60 + us.minute() as i32;
    if (1..=5).contains(&us_day) && in_window(us_min, 570, 960) {
        return StockSession { id: "us", label: "美股指数" };
    }
    StockSession { id: "fut", label: "美股期货" }
}

/// A 股午休（11:30–13:00）时把 portfolio 的 TTL 拉到 13:00，避免午休轮询；
/// 13:00 后恢复 15s。启动时若午休且无缓存，仍会拉一次当天数据。
pub fn stock_poll_ttl_or(normal_secs: u64) -> Duration {
    let cn = Utc::now().with_timezone(&chrono_tz::Asia::Shanghai);
    let day = cn.weekday().num_days_from_sunday() as i32;
    let min = cn.hour() as i32 * 60 + cn.minute() as i32;
    if (1..=5).contains(&day) && (690..780).contains(&min) {
        let secs = (780 - min) * 60 - cn.second() as i32;
        Duration::from_secs(secs.max(1) as u64)
    } else {
        Duration::from_secs(normal_secs)
    }
}

pub fn stock_poll_ttl() -> Duration {
    stock_poll_ttl_or(15)
}

pub fn stock_symbols(cfg: &Value) -> String {
    let id = stock_session().id;
    let pick = match id {
        "cn" => cfg.get("symbols"),
        "us" => cfg.get("us"),
        _ => cfg.get("usFut"),
    };
    pick.and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| cfg.get("symbols").and_then(Value::as_str))
        .unwrap_or("sh000001")
        .to_string()
}

pub fn trading_hours() -> bool {
    let d = Local::now();
    let day = d.weekday().num_days_from_sunday();
    if day == 0 || day == 6 {
        return false;
    }
    let m = d.hour() as i32 * 60 + d.minute() as i32;
    (570..=690).contains(&m) || (780..=900).contains(&m)
}

pub fn minute_slots(symbol: &str) -> f32 {
    if symbol.starts_with("103.") {
        1380.0
    } else if symbol.starts_with("100.") {
        391.0
    } else {
        241.0
    }
}

pub fn fmt_rate(v: f64) -> String {
    let n = v.max(0.0);
    if n < 1024.0 {
        format!("{}B", n.round() as i64)
    } else if n < 1024.0 * 1024.0 {
        format!("{}K", (n / 1024.0).round() as i64)
    } else if n < 1024.0_f64.powi(3) {
        let m = n / 1024.0 / 1024.0;
        if m < 10.0 { format!("{m:.1}M") } else { format!("{m:.0}M") }
    } else {
        format!("{:.1}G", n / 1024.0_f64.powi(3))
    }
}

pub fn net_text(d: &Value) -> String {
    format!(
        "↑{}  ↓{}",
        fmt_rate(d.get("netUp").and_then(Value::as_f64).unwrap_or(0.0)),
        fmt_rate(d.get("netDown").and_then(Value::as_f64).unwrap_or(0.0))
    )
}

pub fn ago(iso: &str) -> String {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return String::new();
    };
    let mins = (Utc::now().signed_duration_since(t.with_timezone(&Utc)).num_minutes()).max(0);
    if mins < 60 {
        format!("{mins}m ago")
    } else if mins < 1440 {
        format!("{}h ago", (mins as f64 / 60.0).round() as i64)
    } else {
        format!("{}d ago", (mins as f64 / 1440.0).round() as i64)
    }
}

pub fn md_text(iso: &str) -> String {
    let parts: Vec<&str> = iso.split('-').collect();
    if parts.len() >= 3 {
        let m = parts[1].parse::<i32>().unwrap_or(0);
        let d = parts[2].parse::<i32>().unwrap_or(0);
        format!("{m}.{d}")
    } else {
        iso.to_string()
    }
}

pub fn date_range(it: &Value) -> String {
    let start = it.get("start").and_then(Value::as_str).unwrap_or("");
    let end = it.get("end").and_then(Value::as_str).unwrap_or("");
    if start == end { md_text(start) } else { format!("{}–{}", md_text(start), md_text(end)) }
}

pub fn minute_chart(
    c: &mut Canvas2D,
    box_: Rect,
    points: &[(f32, f32)],
    pre_close: f64,
    color: Color,
    slots: f32,
) {
    if points.len() < 2 {
        c.set_fill(Color::from_argb(0x2e, 0xff, 0xff, 0xff));
        c.set_font((box_.height() * 0.22).max(10.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(CBaseline::Middle);
        c.fill_text("无分时", box_.left, box_.top + box_.height() / 2.0);
        return;
    }
    let prices: Vec<f64> = points.iter().map(|p| p.1 as f64).collect();
    let mut lo = prices.iter().cloned().fold(f64::INFINITY, f64::min);
    let mut hi = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let pc = if pre_close > 0.0 { pre_close } else { prices[0] as f64 };
    lo = lo.min(pc);
    hi = hi.max(pc);
    let span0 = (hi - lo).max(1e-9);
    lo -= span0 * 0.12;
    hi += span0 * 0.12;
    let span = (prices.len() as f32 - 1.0).max(slots - 1.0).max(1.0);
    let px = |i: usize| box_.left + (i as f32 / span) * box_.width();
    let py = |v: f64| box_.top + box_.height() - (((v - lo) / (hi - lo)) as f32) * box_.height();

    if pre_close > 0.0 {
        c.set_dash(&[3.0, 4.0]);
        c.set_stroke(Color::from_argb(0x24, 0xff, 0xff, 0xff));
        c.set_line_width(1.0);
        c.line(box_.left, py(pc), box_.right, py(pc));
        c.set_dash(&[]);
    }

    let pts: Vec<(f32, f32)> = points.iter().enumerate().map(|(i, p)| (px(i), py(p.1 as f64))).collect();
    let top = Color::from_argb(0x4d, color.r(), color.g(), color.b());
    let bottom = Color::from_argb(0x00, color.r(), color.g(), color.b());
    c.fill_area_gradient(&pts, box_.bottom, top, bottom, box_.top, box_.bottom);
    c.set_stroke(color);
    c.set_line_width((box_.height() * 0.03).max(1.2));
    c.set_line_join_round();
    c.stroke_polyline(&pts);
}

pub fn sparkline(c: &mut Canvas2D, x: f32, y: f32, w: f32, h: f32, data: &[f32], color: Color) {
    c.set_stroke(Color::from_argb(0x0f, 0xff, 0xff, 0xff));
    c.set_line_width(1.0);
    c.line(x, y + h + 0.5, x + w, y + h + 0.5);
    if data.len() < 2 {
        return;
    }
    let dx = w / (HIST_N as f32 - 1.0);
    let px = |i: usize| x + (i as f32 + HIST_N as f32 - data.len() as f32) * dx;
    let py = |v: f32| y + h - v.clamp(0.0, 1.0) * h;
    let pts: Vec<(f32, f32)> = data.iter().enumerate().map(|(i, v)| (px(i), py(*v))).collect();
    let mut area = pts.clone();
    area.push((px(data.len() - 1), y + h));
    area.push((px(0), y + h));
    c.set_fill(Color::from_argb(0x12, color.r(), color.g(), color.b()));
    c.fill_polygon(&area);
    c.set_stroke(color);
    c.set_line_width(2.0);
    c.set_line_join_round();
    c.stroke_polyline(&pts);
}

pub const HIST_N: usize = 60;

pub fn gauge(c: &mut Canvas2D, cx: f32, cy: f32, r: f32, lw: f32, frac: f32, color: Color) {
    c.save();
    c.set_line_cap_round();
    c.set_line_width(lw);
    c.set_stroke(Color::from_argb(0x12, 0xff, 0xff, 0xff));
    c.stroke_arc(cx, cy, r, 135.0, 270.0);
    let f = frac.clamp(0.0, 1.0);
    if f > 0.004 {
        c.set_stroke(color);
        c.stroke_arc(cx, cy, r, 135.0, 270.0 * f);
    }
    c.restore();
}

pub fn seven_seg_width(text: &str, h: f32) -> f32 {
    let w = h * 0.56;
    let gap = h * 0.16;
    let t = h * 0.115;
    let mut total = 0.0;
    for ch in text.chars() {
        if ch == ':' {
            total += t * 2.2 + gap * 0.6;
        } else {
            total += w + gap;
        }
    }
    total - gap
}

pub fn draw_seven_seg(c: &mut Canvas2D, text: &str, x: f32, y: f32, h: f32, color: Color, dim: Color) {
    let t = h * 0.115;
    let w = h * 0.56;
    let gap = h * 0.16;
    let mut cx = x;
    let seg = |c: &mut Canvas2D, sx: f32, sy: f32, sw: f32, sh: f32, on: bool| {
        c.set_fill(if on { color } else { dim });
        c.fill_rrect(Rect::from_xywh(sx, sy, sw, sh), sw.min(sh) / 2.0);
    };
    for ch in text.chars() {
        if ch == ':' {
            c.set_fill(color);
            c.fill_circle(cx + t * 0.6, y + h * 0.32, t * 0.5);
            c.fill_circle(cx + t * 0.6, y + h * 0.68, t * 0.5);
            cx += t * 2.2 + gap * 0.6;
            continue;
        }
        let on = match ch {
            '0' => "abcdef", '1' => "bc", '2' => "abged", '3' => "abgcd", '4' => "fgbc",
            '5' => "afgcd", '6' => "afgedc", '7' => "abc", '8' => "abcdefg", '9' => "abcdfg",
            '-' => "g", _ => "",
        };
        let has = |s: &str| on.contains(s);
        let l = t;
        seg(c, cx + l, y, w - 2.0 * l, l, has("a"));
        seg(c, cx + l, y + h / 2.0 - l / 2.0, w - 2.0 * l, l, has("g"));
        seg(c, cx + l, y + h - l, w - 2.0 * l, l, has("d"));
        seg(c, cx, y + l, l, h / 2.0 - 1.5 * l, has("f"));
        seg(c, cx + w - l, y + l, l, h / 2.0 - 1.5 * l, has("b"));
        seg(c, cx, y + h / 2.0 + l / 2.0, l, h / 2.0 - 1.5 * l, has("e"));
        seg(c, cx + w - l, y + h / 2.0 + l / 2.0, l, h / 2.0 - 1.5 * l, has("c"));
        cx += w + gap;
    }
}

pub fn news_sources(cfg: &Value) -> Vec<&'static str> {
    let all: Vec<&'static str> = NEWS_TABS.iter().map(|(id, _)| *id).collect();
    let Some(arr) = cfg.get("sources").and_then(Value::as_array) else {
        return all;
    };
    arr.iter()
        .filter_map(Value::as_str)
        .filter_map(|id| NEWS_TABS.iter().find(|(k, _)| *k == id).map(|(k, _)| *k))
        .collect()
}

pub fn news_label(cfg: &Value, id: &str) -> String {
    if let Some(labels) = cfg.get("labels").and_then(Value::as_object) {
        if let Some(v) = labels.get(id).and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
            return v.to_string();
        }
    }
    NEWS_TABS
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, v)| *v)
        .unwrap_or(id)
        .to_string()
}

pub const NEWS_TABS: &[(&str, &str)] = &[
    ("toutiao", "今日头条"), ("baidu", "百度"), ("zhihu", "知乎"), ("weibo", "微博"),
    ("tieba", "贴吧"), ("v2ex", "V2EX"), ("juejin", "掘金"), ("sspai", "少数派"),
    ("ithome", "IT之家"), ("36kr", "36氪"), ("wscn", "华尔街见闻"), ("hackernews", "HN"),
];

pub fn parse_points(v: &Value) -> Vec<(f32, f32)> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let a = p.as_array()?;
                    Some((a.first()?.as_str()?.to_string(), a.get(1)?.as_f64()?))
                })
                .filter_map(|(_, v)| Some((0.0, v as f32)))
                .collect()
        })
        .unwrap_or_default()
}

pub fn stock_row(c: &mut Canvas2D, r: Rect, item: &Value, t: &Theme, card: bool, slots: Option<f32>) {
    let pad = if card { (r.height() * 0.14).max(8.0) } else { 0.0 };
    if card {
        c.set_fill(t.card_hi);
        c.fill_rrect(r, (r.height() * 0.14).min(8.0));
    }
    let price = item.get("price").and_then(Value::as_f64);
    let prev = item.get("prev").and_then(Value::as_f64);
    let has_quote = price.is_some() && prev.is_some();
    let pct = item.get("pct").and_then(Value::as_f64).unwrap_or(0.0);
    let up = pct >= 0.0;
    let color = if has_quote { if up { t.up } else { t.down } } else { t.fg4 };
    let name_w = (r.width() * 0.36).min((r.width() * 0.3).max(96.0));
    c.set_align(Align::Left);
    c.set_baseline(CBaseline::Top);
    c.set_fill(t.fg2);
    c.set_font((r.height() * 0.22).clamp(12.0, 26.0), 500, false);
    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
    let name = if name.is_empty() { item.get("symbol").and_then(Value::as_str).unwrap_or("") } else { name };
    let name = c.ellipsis(name, name_w);
    c.fill_text(&name, r.left + pad, r.top + r.height() * 0.22);
    c.set_fill(t.fg4);
    c.set_font((r.height() * 0.15).clamp(9.0, 17.0), 400, true);
    c.fill_text(
        &item.get("symbol").and_then(Value::as_str).unwrap_or("").to_uppercase(),
        r.left + pad,
        r.top + r.height() * 0.58,
    );
    let price_w = (r.width() * 0.26).min((r.width() * 0.22).max(84.0));
    let price_txt = price.map(|p| format!("{p:.2}")).unwrap_or_else(|| "—".into());
    let pct_txt = if has_quote { format!("{}{:.2}%", if up { "+" } else { "" }, pct) } else { String::new() };
    c.set_align(Align::Right);
    c.set_fill(color);
    let size = c.fit_font(&price_txt, price_w, (r.height() * 0.3).min(34.0), 400, true);
    c.set_font(size, 400, true);
    c.fill_text(&price_txt, r.right - pad, r.top + r.height() * 0.2);
    c.set_font((r.height() * 0.19).clamp(10.0, 22.0), 500, true);
    c.fill_text(&pct_txt, r.right - pad, r.top + r.height() * 0.58);
    let sx = r.left + pad + name_w + pad * 0.6;
    let sw = r.right - pad - price_w - pad * 0.6 - sx;
    if sw > 30.0 {
        let points = parse_points(item.get("points").unwrap_or(&Value::Null));
        let pre = item.get("preClose").and_then(Value::as_f64)
            .or_else(|| item.get("prev").and_then(Value::as_f64))
            .unwrap_or(0.0);
        minute_chart(
            c,
            Rect::from_xywh(sx, r.top + r.height() * 0.2, sw, r.height() * 0.62),
            &points,
            pre,
            color,
            slots.unwrap_or(points.len() as f32),
        );
    }

    // 本时段最高/最低：小字放在分时图下方，不抢价格和曲线
    let high = item.get("high").and_then(Value::as_f64);
    let low = item.get("low").and_then(Value::as_f64);
    if high.is_some() || low.is_some() {
        c.set_baseline(CBaseline::Top);
        c.set_fill(t.fg4);
        c.set_font((r.height() * 0.12).clamp(9.0, 14.0), 400, true);
        if let Some(high) = high {
            c.set_align(Align::Left);
            c.fill_text(&format!("高 {high:.2}"), r.left + pad, r.top + r.height() * 0.84);
        }
        if let Some(low) = low {
            c.set_align(Align::Right);
            c.fill_text(&format!("低 {low:.2}"), r.right - pad, r.top + r.height() * 0.84);
        }
    }
}
