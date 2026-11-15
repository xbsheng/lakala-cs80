use std::time::Duration;

use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{big_days, card, dashed, date_range};

pub fn needs(_cfg: &Value) -> Vec<Need> {
    vec![Need::new("holiday", vec![], Duration::from_secs(1800))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let pad = (r.width() * 0.05).max(10.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("");
    let inner = card(c, r, t, title, "", t.fg4, pad);
    let Some(items) = d.get("upcoming").and_then(Value::as_array) else {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.12).clamp(12.0, 20.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.fill_text(if d.is_null() { "加载中…" } else { "暂无数据" }, inner.rect.left, inner.rect.top + inner.rect.height() * 0.05);
        return;
    };
    if items.is_empty() {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.12).clamp(12.0, 20.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.fill_text("暂无数据", inner.rect.left, inner.rect.top + inner.rect.height() * 0.05);
        return;
    }
    let next = &items[0];
    let ongoing = next.get("ongoing").and_then(Value::as_bool).unwrap_or(false);
    let n = if ongoing {
        next.get("left").and_then(Value::as_i64).unwrap_or(0)
    } else {
        next.get("until").and_then(Value::as_i64).unwrap_or(0)
    };
    let name = next.get("name").and_then(Value::as_str).unwrap_or("");
    let label = if ongoing { format!("{name}·假期中") } else { format!("{name}还有") };

    if inner.rect.height() > inner.rect.width() * 1.15 {
        let row_h = (inner.rect.width() * 0.36).clamp(66.0, 120.0);
        let rows = ((inner.rect.height() / row_h).floor() as usize).max(1).min(items.len());
        let h = row_h.min(inner.rect.height() / rows as f32);
        for (i, it) in items.iter().take(rows).enumerate() {
            dashed(c, inner.rect.left, inner.rect.top + i as f32 * h, inner.rect.right, inner.rect.top + i as f32 * h);
            holiday_row(c, inner.rect.left, inner.rect.top + i as f32 * h, inner.rect.width(), h, it, t);
        }
        dashed(c, inner.rect.left, inner.rect.top + rows as f32 * h, inner.rect.right, inner.rect.top + rows as f32 * h);
        return;
    }

    if inner.rect.width() >= 520.0 && inner.rect.height() >= 210.0 {
        let left_w = inner.rect.width() * 0.44;
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Middle);
        c.set_fill(t.fg3);
        c.set_font((inner.rect.height() * 0.14).clamp(12.0, inner.rect.width() * 0.05), 500, false);
        c.fill_text(&label, inner.rect.left, inner.rect.top + inner.rect.height() * 0.17);
        big_days(
            c,
            inner.rect.left,
            inner.rect.top + inner.rect.height() * 0.62,
            left_w * 0.96,
            (inner.rect.height() * 0.34).min(left_w * 0.62),
            n,
            t,
            Align::Left,
            "天",
        );
        c.set_fill(t.fg3);
        c.set_font((inner.rect.height() * 0.13).clamp(11.0, inner.rect.width() * 0.04), 400, false);
        c.fill_text(&date_range(next), inner.rect.left, inner.rect.top + inner.rect.height() * 0.86);

        let rest = &items[1..];
        let x0 = inner.rect.left + left_w + inner.rect.width() * 0.03;
        let w_r = inner.rect.right - x0;
        let row_h = (inner.rect.height() * 0.24).clamp(56.0, 96.0);
        let rows = ((inner.rect.height() / row_h).floor() as usize).max(1).min(rest.len());
        let h = row_h.min(inner.rect.height() / rows as f32);
        for (i, it) in rest.iter().take(rows).enumerate() {
            dashed(c, x0, inner.rect.top + i as f32 * h, x0 + w_r, inner.rect.top + i as f32 * h);
            holiday_row(c, x0, inner.rect.top + i as f32 * h, w_r, h, it, t);
        }
        dashed(c, x0, inner.rect.top + rows as f32 * h, x0 + w_r, inner.rect.top + rows as f32 * h);
        return;
    }

    if inner.rect.width() > inner.rect.height() * 1.25 {
        let name_size = (inner.rect.height() * 0.24).clamp(13.0, inner.rect.width() * 0.07);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Middle);
        c.set_fill(t.fg2);
        c.set_font(name_size, 500, false);
        c.fill_text(name, inner.rect.left, inner.rect.top + inner.rect.height() * 0.34);
        c.set_fill(t.fg3);
        c.set_font((name_size * 0.82).max(11.0), 400, false);
        c.fill_text(&date_range(next), inner.rect.left, inner.rect.top + inner.rect.height() * 0.7);
        big_days(
            c,
            inner.rect.right,
            inner.rect.top + inner.rect.height() * 0.68,
            inner.rect.width() * 0.62,
            (inner.rect.height() * 0.5).min(inner.rect.width() * 0.3),
            n,
            t,
            Align::Right,
            "天",
        );
        return;
    }

    let name_size = (inner.rect.width() * 0.17).min(inner.rect.height() * 0.16).max(11.0);
    c.set_align(Align::Center);
    c.set_baseline(Baseline::Middle);
    c.set_fill(t.fg3);
    c.set_font(name_size, 500, false);
    c.fill_text(if ongoing { &label } else { name }, inner.rect.left + inner.rect.width() / 2.0, inner.rect.top + inner.rect.height() * 0.3);
    big_days(
        c,
        inner.rect.left + inner.rect.width() / 2.0,
        inner.rect.top + inner.rect.height() * 0.72,
        inner.rect.width() * 0.9,
        (inner.rect.width() * 0.5).min(inner.rect.height() * 0.42),
        n,
        t,
        Align::Center,
        "",
    );
}

fn holiday_row(c: &mut Canvas2D, x: f32, y: f32, w: f32, h: f32, it: &Value, t: &Theme) {
    let v = if it.get("ongoing").and_then(Value::as_bool).unwrap_or(false) {
        it.get("left").and_then(Value::as_i64).unwrap_or(0)
    } else {
        it.get("until").and_then(Value::as_i64).unwrap_or(0)
    };
    let cy = y + h / 2.0;
    let num_size = (h * 0.5).min(48.0);
    let suffix = if w >= 200.0 { "天" } else { "" };
    c.set_font(num_size * 0.42, 500, false);
    let num_w = if suffix.is_empty() { 0.0 } else { c.measure_text(suffix) + num_size * 0.1 };
    c.set_font(num_size, 700, false);
    let avail = w - (num_w + c.measure_text(&v.to_string())) - h * 0.16;
    let range = date_range(it);
    let name = it.get("name").and_then(Value::as_str).unwrap_or("");
    let name_size = (h * 0.4).clamp(13.0, 34.0);
    let width_of = |c: &mut Canvas2D, ns: f32| {
        c.set_font(ns, 500, false);
        let nw = c.measure_text(name);
        c.set_font(ns * 0.72, 400, false);
        nw + ns * 0.45 + c.measure_text(&range)
    };
    let mut ns = name_size;
    while width_of(c, ns) > avail && ns > 13.0 {
        ns *= 0.94;
    }
    let with_date = width_of(c, ns) <= avail;
    c.set_align(Align::Left);
    c.set_baseline(Baseline::Middle);
    c.set_font(ns, 500, false);
    c.set_fill(t.fg2);
    c.fill_text(name, x, cy);
    if with_date {
        let nw = c.measure_text(name);
        c.set_font(ns * 0.72, 400, false);
        c.set_fill(t.fg3);
        c.fill_text(&range, x + nw + ns * 0.45, cy);
    }
    big_days(
        c,
        x + w,
        cy + (h * 0.19).min(18.0),
        w * 0.58,
        num_size,
        v,
        t,
        Align::Right,
        suffix,
    );
    let _ = Color::TRANSPARENT;
}
