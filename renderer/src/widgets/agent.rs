use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::assets::Assets;
use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{card, rows_for};

pub fn needs(_cfg: &Value) -> Vec<Need> {
    vec![Need::new("agents", vec![], Duration::from_secs(3))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme, assets: &mut Assets) {
    let hide: Vec<&str> = cfg.get("hide").and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let hide_tasks: Vec<&str> = cfg.get("hideTasks").and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let items: Vec<&Value> = d.get("items").and_then(Value::as_array)
        .map(|a| a.iter().filter(|it| {
            let agent = it.get("agent").and_then(Value::as_str).unwrap_or("");
            let id = it.get("id").and_then(Value::as_str).unwrap_or("");
            !hide.contains(&agent) && !hide_tasks.contains(&id)
        }).collect()).unwrap_or_default();
    let running = items.iter().filter(|it| it.get("state").and_then(Value::as_str) == Some("running")).count();
    let meta = if items.is_empty() { String::new() } else if running > 0 { format!("{running} 运行中") } else { format!("{} 会话", items.len()) };
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("Agent");
    let pad = (r.width() * 0.03).max(10.0);
    let inner = card(c, r, t, title, &meta, t.fg4, pad);
    if items.is_empty() {
        let total = d.get("items").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.14).max(13.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        let msg = if d.get("error").is_some() {
            "扫描失败"
        } else if total > 0 {
            "已在配置里隐藏全部 Agent"
        } else {
            "没有检测到 Agent 会话"
        };
        c.fill_text(msg, inner.rect.left, inner.rect.top);
        return;
    }
    let now = now_ms();
    let pulse = 0.45 + 0.55 * ((now as f64 % 1600.0) / 1600.0 * std::f64::consts::PI).sin().abs() as f32;
    let row0 = (inner.rect.width() * 0.085).clamp(40.0, 96.0);
    let (rows, row_h) = rows_for(inner.rect.height(), row0, items.len());
    let logo = (row_h * 0.62).clamp(16.0, 34.0);
    let dot = (row_h * 0.16).clamp(5.0, 10.0);
    let chip_w = (row_h * 1.5).max(52.0);
    for (i, it) in items.iter().take(rows).enumerate() {
        let y = inner.rect.top + i as f32 * row_h;
        let cy = y + row_h / 2.0;
        // 档位/弱化/呼吸/右侧文案都由 host 的 display 给出（语义单点），
        // 这里只把 tone 映射成主题色。
        let disp = it.get("display");
        let field = |k: &str| disp.and_then(|d| d.get(k));
        let tone = field("tone").and_then(Value::as_str).unwrap_or("");
        let color = match tone {
            "up" => Color::from_rgb(0x3d, 0xdc, 0x84),
            "warn" => Color::from_rgb(0xfa, 0xbe, 0x46),
            "bad" => Color::from_rgb(0xef, 0x6f, 0x6f),
            "info" => Color::from_rgb(0x5a, 0xa9, 0xff),
            _ => Color::from_rgb(0x6b, 0x72, 0x80),
        };
        let label = field("label").and_then(Value::as_str).unwrap_or("");
        let right = field("right").and_then(Value::as_str).unwrap_or("");
        let blink = field("blink").and_then(Value::as_bool).unwrap_or(false);
        let gray = field("muted").and_then(Value::as_bool).unwrap_or(true);
        let alpha = if blink { pulse } else if gray { 0.45 } else { 1.0 };
        c.set_alpha(alpha);
        c.set_fill(color);
        c.fill_circle(inner.rect.left + dot / 2.0, cy, dot / 2.0);
        c.set_alpha(if gray { 0.4 } else { 1.0 });
        let lx = inner.rect.left + dot + 8.0;
        let agent = it.get("agent").and_then(Value::as_str).unwrap_or("");
        if !assets.draw_logo(c, agent, lx, cy - logo / 2.0, logo, logo) {
            c.set_fill(color);
            c.fill_rrect(Rect::from_xywh(lx, cy - logo / 2.0, logo, logo), 4.0);
            c.set_fill(Color::from_rgb(0x14, 0x15, 0x19));
            c.set_font(logo * 0.62, 700, false);
            c.set_align(Align::Center);
            c.set_baseline(Baseline::Middle);
            let letter = it.get("short").and_then(Value::as_str)
                .or_else(|| it.get("name").and_then(Value::as_str))
                .and_then(|s| s.chars().next()).map(|ch| ch.to_uppercase().to_string()).unwrap_or_else(|| "A".into());
            c.fill_text(&letter, lx + logo / 2.0, cy + 1.0);
        }
        c.set_alpha(1.0);
        let title_x = lx + logo + (row_h * 0.2).max(8.0);
        let title_w = inner.rect.right - title_x - chip_w - 8.0;
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Middle);
        c.set_fill(if gray { t.fg4 } else if tone == "bad" || tone == "info" { t.fg2 } else { t.fg });
        c.set_font((row_h * 0.36).clamp(14.0, 24.0), 400, false);
        let title = it.get("title").and_then(Value::as_str).unwrap_or("");
        let title = c.ellipsis(title, title_w);
        c.fill_text(&title, title_x, cy);
        c.set_align(Align::Right);
        c.set_font(
            (row_h * 0.3).clamp(12.0, 19.0),
            if label.is_empty() { 400 } else { 500 },
            label.is_empty(),
        );
        c.set_alpha(if blink { pulse } else { 1.0 });
        c.set_fill(color);
        c.fill_text(right, inner.rect.right, cy);
        c.set_alpha(1.0);
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
