use std::time::Duration;

use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{card, fresh_stamp, news_label, news_sources, now_ms};

pub fn needs(cfg: &Value) -> Vec<Need> {
    let sources = news_sources(cfg).join(",");
    vec![Need::new("news", vec![("sources".into(), sources)], Duration::from_secs(180))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let pad = (r.width() * 0.028).max(10.0);
    let inner = card(c, r, t, "", "", t.fg4, pad);
    let ids = news_sources(cfg);
    if ids.is_empty() {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.09).clamp(13.0, 20.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.fill_text("未选择数据源", inner.rect.left, inner.rect.top + inner.rect.height() * 0.44);
        return;
    }
    let mut tab_font = (inner.rect.height() * 0.1).min(inner.rect.width() * 0.034).clamp(13.0, 24.0);
    let labels: Vec<String> = ids.iter().map(|id| news_label(cfg, id)).collect();
    let strip_w = |c: &mut Canvas2D, f: f32| -> f32 {
        c.set_font(f, 500, false);
        labels.iter().map(|s| c.measure_text(s) + f * 1.5).sum()
    };
    while strip_w(c, tab_font) > inner.rect.width() && tab_font > 11.0 {
        tab_font *= 0.94;
    }
    let tab_h = tab_font * 1.9;
    let rotate = cfg.get("rotate").and_then(Value::as_i64).unwrap_or(60).max(5);
    let active = ((now_ms() / (rotate * 1000)) as usize) % ids.len();
    c.set_baseline(Baseline::Middle);
    c.set_font(tab_font, 500, false);
    let gap = tab_font * 0.3;
    let tabs: Vec<f32> = labels.iter().map(|s| c.measure_text(s) + tab_font * 1.2).collect();
    let total: f32 = tabs.iter().sum::<f32>() + gap * (tabs.len() as f32 - 1.0);
    let (a, b) = if total > inner.rect.width() {
        let mut a = active;
        let mut b = active;
        let mut used = tabs[active];
        let mut l = active as isize - 1;
        let mut r = active + 1;
        loop {
            if r < tabs.len() && used + tabs[r] + gap <= inner.rect.width() {
                used += tabs[r] + gap;
                b = r;
                r += 1;
            } else if l >= 0 && used + tabs[l as usize] + gap <= inner.rect.width() {
                used += tabs[l as usize] + gap;
                a = l as usize;
                l -= 1;
            } else {
                break;
            }
        }
        (a, b)
    } else {
        (0, tabs.len() - 1)
    };
    let mut tx = inner.rect.left;
    for i in a..=b {
        let w = tabs[i];
        if i == active {
            c.set_fill(Color::from_argb(0x1a, 0xff, 0xff, 0xff));
            c.fill_rrect(Rect::from_xywh(tx, inner.rect.top, w, tab_h), tab_h * 0.28);
            c.set_fill(t.fg);
        } else {
            c.set_fill(t.fg3);
        }
        c.set_align(Align::Center);
        c.fill_text(&labels[i], tx + w / 2.0, inner.rect.top + tab_h / 2.0);
        tx += w + gap;
    }

    let stamp = if d.get("stale").and_then(Value::as_bool).unwrap_or(false) {
        Some(("更新失败·重取中".to_string(), Color::from_rgb(0xfa, 0xbe, 0x46)))
    } else {
        fresh_stamp(d)
    };
    if let Some((text, color)) = stamp {
        c.set_font(tab_font * 0.78, 500, false);
        let tw = c.measure_text(&text);
        if tx + tw + tab_font * 0.6 < inner.rect.right {
            c.set_align(Align::Right);
            c.set_fill(if color == Color::TRANSPARENT { t.fg4 } else { color });
            c.fill_text(&text, inner.rect.right, inner.rect.top + tab_h / 2.0);
        }
    }

    let list_y = inner.rect.top + tab_h + tab_font * 0.5;
    let list_h = inner.rect.bottom - list_y;
    let items = d.get(ids[active]).and_then(Value::as_array);
    let Some(items) = items else {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.08).clamp(13.0, 20.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Middle);
        c.fill_text("加载中…", inner.rect.left, list_y + list_h.min(24.0) / 2.0);
        return;
    };
    if items.is_empty() {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.08).clamp(13.0, 20.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Middle);
        c.fill_text("暂无数据", inner.rect.left, list_y + list_h.min(24.0) / 2.0);
        return;
    }
    let row_font = (inner.rect.width() * 0.028).clamp(13.0, 24.0);
    let rows = ((list_h / (row_font * 1.9)).floor() as usize).max(1);
    let row_h = (list_h / rows as f32).min(row_font * 2.4);
    let rank_w = row_font * 1.6;
    c.set_font(row_font * 0.88, 400, true);
    let hot_w = items.iter().take(rows)
        .filter_map(|it| it.get("hot").and_then(Value::as_i64))
        .map(|v| c.measure_text(&v.to_string()))
        .fold(0.0, f32::max);
    let title_w = inner.rect.width() - rank_w - hot_w - row_font * if hot_w > 0.0 { 1.4 } else { 0.0 };
    for (i, it) in items.iter().take(rows).enumerate() {
        let cy = list_y + i as f32 * row_h + row_h / 2.0;
        c.set_baseline(Baseline::Middle);
        c.set_font(row_font * 0.88, 600, true);
        c.set_fill(t.fg4);
        c.set_align(Align::Left);
        c.fill_text(&(i + 1).to_string(), inner.rect.left, cy);
        c.set_font(row_font, 400, false);
        c.set_fill(t.fg2);
        let title = it.get("title").and_then(Value::as_str).unwrap_or("");
        let title = c.ellipsis(title, (title_w).max(20.0));
        c.fill_text(&title, inner.rect.left + rank_w, cy);
        if let Some(hot) = it.get("hot").and_then(Value::as_i64) {
            if hot_w > 0.0 {
                c.set_font(row_font * 0.88, 400, true);
                c.set_fill(t.fg4);
                c.set_align(Align::Right);
                c.fill_text(&hot.to_string(), inner.rect.right, cy);
            }
        }
    }
}
