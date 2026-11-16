use std::time::Duration;

use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::card;

pub fn needs(cfg: &Value) -> Vec<Need> {
    let mut ids = Vec::new();
    for key in [
        "temp", "humidity", "air", "climate", "lights", "switches", "doors", "lock", "power",
        "energy", "media",
    ] {
        if let Some(s) = cfg.get(key).and_then(Value::as_str) {
            for part in s.split(',') {
                let id = part.trim();
                if !id.is_empty() && !ids.iter().any(|x: &String| x == id) {
                    ids.push(id.to_string());
                }
            }
        }
    }
    if ids.is_empty() {
        return Vec::new();
    }
    vec![Need::new(
        "ha",
        vec![("entities".to_string(), ids.join(","))],
        Duration::from_millis(2500),
    )]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let items = d.get("items").and_then(Value::as_object);
    let online = d.get("online").and_then(Value::as_bool).unwrap_or(false)
        && !d.get("stale").and_then(Value::as_bool).unwrap_or(false);
    let pad = (r.width() * 0.024).max(12.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("家里");
    let meta = if d.is_null() {
        "HA …".to_string()
    } else if online {
        "HA 在线".to_string()
    } else {
        "HA 离线".to_string()
    };
    let inner = card(
        c,
        r,
        t,
        title,
        &meta,
        if online { t.fg4 } else { t.up },
        pad,
    );

    let Some(items) = items else {
        empty(c, &inner.rect, t, d);
        return;
    };
    if items.is_empty() {
        empty(c, &inner.rect, t, d);
        return;
    }
    let get = |id: &str| items.get(id);
    let ids_of = |key: &str| -> Vec<&Value> {
        cfg.get(key)
            .and_then(Value::as_str)
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .filter_map(|id| get(id))
                    .collect()
            })
            .unwrap_or_default()
    };

    let wide = inner.rect.width() > 460.0;
    let metric_keys: &[&str] = if wide {
        &["temp", "humidity", "air"]
    } else {
        &["temp", "humidity"]
    };
    let metric_h = inner.rect.height() * if wide { 0.5 } else { 0.54 };
    let col_w = inner.rect.width() / metric_keys.len() as f32;
    for (i, key) in metric_keys.iter().enumerate() {
        let x = inner.rect.left + i as f32 * col_w;
        let it = cfg.get(*key).and_then(Value::as_str).and_then(get);
        let mut value = "—".to_string();
        let mut unit = String::new();
        let mut label = match *key {
            "temp" => "室内温度",
            "humidity" => "室内湿度",
            _ => "空气质量",
        }
        .to_string();
        if let Some(it) = it {
            if it.get("domain").and_then(Value::as_str) == Some("weather") {
                let a = it.get("attributes");
                value = a
                    .and_then(|a| a.get("temperature"))
                    .and_then(Value::as_f64)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".to_string());
                unit = "°C".to_string();
                label = format!("室外 · {}", ha_text(it));
            } else {
                value = ha_text(it);
                unit = it.get("unit").and_then(Value::as_str).unwrap_or("").to_string();
            }
        }
        let ok = it
            .map(|it| {
                let s = it.get("state").and_then(Value::as_str).unwrap_or("");
                s != "unavailable" && s != "unknown"
            })
            .unwrap_or(false);
        ha_metric(
            c,
            x,
            inner.rect.top + 2.0,
            col_w - 14.0,
            metric_h,
            &label,
            &value,
            &unit,
            if ok { t.fg } else { t.fg4 },
            t,
        );
        if i > 0 {
            c.set_stroke(Color::from_argb(0x0f, 0xff, 0xff, 0xff));
            c.set_line_width(1.0);
            c.line(
                x - 7.5,
                inner.rect.top + metric_h * 0.12,
                x - 7.5,
                inner.rect.top + metric_h * 0.9,
            );
        }
    }

    let wanted: Vec<&str> = cfg
        .get("chips")
        .and_then(Value::as_str)
        .unwrap_or("climate,lights,switches,media")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut chips: Vec<(String, String, Color)> = Vec::new();
    for role in wanted {
        match role {
            "climate" => {
                if let Some(it) = cfg.get("climate").and_then(Value::as_str).and_then(get) {
                    let target = it
                        .get("attributes")
                        .and_then(|a| a.get("temperature"))
                        .and_then(Value::as_f64)
                        .map(|v| format!(" · {v}°"))
                        .unwrap_or_default();
                    chips.push((
                        "空调".into(),
                        format!("{}{}", ha_text(it), target),
                        if it.get("state").and_then(Value::as_str) == Some("off") {
                            t.fg4
                        } else {
                            t.accent
                        },
                    ));
                }
            }
            "lights" | "switches" => {
                let list = ids_of(role);
                if list.is_empty() {
                    continue;
                }
                let on = list
                    .iter()
                    .filter(|it| it.get("state").and_then(Value::as_str) == Some("on"))
                    .count();
                let text = if list.len() == 1 {
                    ha_text(list[0])
                } else if on == 0 {
                    "全关".into()
                } else if on == list.len() {
                    "全开".into()
                } else {
                    format!("{on}/{} 开", list.len())
                };
                chips.push((
                    if role == "lights" { "灯" } else { "电器" }.into(),
                    text,
                    if on > 0 { t.accent } else { t.fg4 },
                ));
            }
            "doors" => {
                let list = ids_of("doors");
                if list.is_empty() {
                    continue;
                }
                let open = list
                    .iter()
                    .filter(|it| it.get("state").and_then(Value::as_str) == Some("on"))
                    .count();
                chips.push((
                    "门窗".into(),
                    if open > 0 {
                        format!("{open} 开")
                    } else {
                        "全关".into()
                    },
                    if open > 0 { t.up } else { t.down },
                ));
            }
            "lock" => {
                if let Some(it) = cfg.get("lock").and_then(Value::as_str).and_then(get) {
                    let bad = it.get("state").and_then(Value::as_str) != Some("locked");
                    chips.push(("门锁".into(), ha_text(it), if bad { t.up } else { t.down }));
                }
            }
            "media" => {
                if let Some(it) = cfg.get("media").and_then(Value::as_str).and_then(get) {
                    let title = it
                        .get("attributes")
                        .and_then(|a| a.get("media_title"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let playing = it.get("state").and_then(Value::as_str) == Some("playing");
                    chips.push((
                        "音箱".into(),
                        if playing && !title.is_empty() {
                            title.to_string()
                        } else {
                            ha_text(it)
                        },
                        if playing { t.accent } else { t.fg4 },
                    ));
                }
            }
            _ => {}
        }
    }

    let chip_y = inner.rect.top + metric_h + 8.0;
    let chip_h = (inner.rect.height() - metric_h - 34.0).clamp(36.0, 54.0);
    if inner.rect.height() >= 170.0 && !chips.is_empty() {
        let gap = 8.0;
        let chip_w = (inner.rect.width() - gap * (chips.len() as f32 - 1.0)) / chips.len() as f32;
        for (i, (label, text, color)) in chips.iter().enumerate() {
            ha_chip(
                c,
                inner.rect.left + i as f32 * (chip_w + gap),
                chip_y,
                chip_w,
                chip_h,
                label,
                text,
                *color,
                t,
            );
        }
    }

    let power = cfg.get("power").and_then(Value::as_str).and_then(get);
    let energy = cfg.get("energy").and_then(Value::as_str).and_then(get);
    let mut foot = Vec::new();
    if let Some(it) = power {
        let unit = it.get("unit").and_then(Value::as_str).unwrap_or("");
        foot.push(format!("功率 {} {}", ha_text(it), unit).trim_end().to_string());
    }
    if let Some(it) = energy {
        let unit = it.get("unit").and_then(Value::as_str).unwrap_or("");
        foot.push(format!("今日 {} {}", ha_text(it), unit).trim_end().to_string());
    }
    if foot.is_empty() {
        foot.push("Home Assistant".into());
    }
    let foot_y = inner.rect.bottom;
    c.set_baseline(Baseline::Bottom);
    c.set_align(Align::Left);
    c.set_font((inner.rect.height() * 0.06).clamp(11.0, 14.0), 400, false);
    c.set_fill(t.fg3);
    let foot_text = c.ellipsis(&foot.join("   "), inner.rect.width() * 0.9);
    c.fill_text(&foot_text, inner.rect.left, foot_y);
}

fn empty(c: &mut Canvas2D, inner: &Rect, t: &Theme, d: &Value) {
    c.set_align(Align::Center);
    c.set_baseline(Baseline::Middle);
    c.set_fill(t.fg4);
    c.set_font((inner.height() * 0.12).clamp(13.0, 20.0), 400, false);
    let msg = d
        .get("error")
        .and_then(Value::as_str)
        .map(|e| format!("HA：{e}"))
        .unwrap_or_else(|| "Home Assistant 未配置".into());
    let msg = c.ellipsis(&msg, inner.width() * 0.92);
    c.fill_text(&msg, inner.left + inner.width() / 2.0, inner.top + inner.height() / 2.0);
}

fn ha_metric(
    c: &mut Canvas2D,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: &str,
    value: &str,
    unit: &str,
    color: Color,
    t: &Theme,
) {
    c.set_align(Align::Left);
    c.set_baseline(Baseline::Top);
    c.set_font((h * 0.13).clamp(11.0, 14.0), 600, false);
    c.set_fill(t.fg4);
    c.tracked_text(label, x, y, (h * 0.13).clamp(11.0, 14.0) * 0.1);
    let size = c.fit_font(value, w - 4.0, (h * 0.58).min(64.0), 300, value.is_ascii());
    c.set_font(size, 300, value.is_ascii());
    c.set_fill(color);
    c.set_baseline(Baseline::Alphabetic);
    let base = y + h * 0.78;
    c.fill_text(value, x, base);
    if !unit.is_empty() {
        let vw = c.measure_text(value);
        c.set_font((size * 0.4).max(13.0), 400, unit.is_ascii());
        c.set_fill(t.fg3);
        c.fill_text(unit, x + vw + 5.0, base);
    }
}

fn ha_chip(
    c: &mut Canvas2D,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: &str,
    text: &str,
    color: Color,
    t: &Theme,
) {
    c.set_fill(Color::from_argb(0x09, 0xff, 0xff, 0xff));
    c.fill_rrect(Rect::from_xywh(x, y, w, h), 7.0);
    c.set_stroke(Color::from_argb(0x0f, 0xff, 0xff, 0xff));
    c.set_line_width(1.0);
    c.stroke_rrect(Rect::from_xywh(x, y, w, h), 7.0);
    let dot = (h * 0.11).clamp(3.5, 5.0);
    c.set_fill(color);
    c.fill_circle(x + 12.0, y + h / 2.0, dot);
    c.set_align(Align::Left);
    c.set_baseline(Baseline::Middle);
    let size = (h * 0.33).clamp(12.0, 17.0);
    let tx = x + 12.0 + dot + 7.0;
    c.set_font(size * 0.84, 600, false);
    c.set_fill(t.fg4);
    c.fill_text(label, tx, y + h / 2.0 - size * 0.52);
    c.set_font(size, 500, false);
    c.set_fill(t.fg2);
    let text = c.ellipsis(text, w - (tx - x) - 8.0);
    c.fill_text(&text, tx, y + h / 2.0 + size * 0.52);
}

fn ha_text(it: &Value) -> String {
    let s = it.get("state").and_then(Value::as_str).unwrap_or("");
    if s.is_empty() || s == "unknown" || s == "unavailable" {
        return "—".into();
    }
    let dom = it.get("domain").and_then(Value::as_str).unwrap_or("");
    match dom {
        "climate" => match s {
            "off" => "关",
            "cool" => "制冷",
            "heat" => "制热",
            "dry" => "除湿",
            "fan_only" => "送风",
            "auto" => "自动",
            _ => s,
        }
        .into(),
        "media_player" => match s {
            "playing" => "播放中",
            "paused" => "暂停",
            "idle" => "待机",
            "off" => "关",
            "on" => "开",
            "buffering" => "缓冲",
            _ => s,
        }
        .into(),
        "lock" => match s {
            "locked" => "已锁",
            "unlocked" => "未锁",
            "locking" => "上锁中",
            "unlocking" => "开锁中",
            "jammed" => "卡住",
            _ => s,
        }
        .into(),
        "weather" => match s {
            "clear" | "sunny" => "晴",
            "partlycloudy" => "多云",
            "cloudy" => "阴",
            "fog" => "雾",
            "hail" => "冰雹",
            "lightning" => "雷雨",
            "rainy" => "雨",
            "pouring" => "暴雨",
            "snowy" => "雪",
            "windy" => "大风",
            "exceptional" => "异常",
            _ => s,
        }
        .into(),
        "binary_sensor" | "input_boolean" | "light" | "switch" => match s {
            "on" => "开",
            "off" => "关",
            _ => s,
        }
        .into(),
        _ => s.into(),
    }
}
