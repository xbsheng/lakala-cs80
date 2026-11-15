use std::time::Duration;

use serde_json::Value;
use skia_safe::Rect;

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::{ago, card};

pub fn needs(cfg: &Value) -> Vec<Need> {
    let repo = cfg.get("repo").and_then(Value::as_str).unwrap_or("tauri-apps/tauri");
    vec![Need::new("github", vec![("repo".into(), repo.into())], Duration::from_secs(300))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let pad = (r.width() * 0.06).max(10.0);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("GitHub");
    let meta = d.get("pushed").and_then(Value::as_str).map(ago).unwrap_or_default();
    let inner = card(c, r, t, title, &meta, t.fg4, pad);
    let Some(stars) = d.get("stars").and_then(Value::as_i64) else {
        c.set_fill(t.fg4);
        c.set_font((inner.rect.height() * 0.16).max(13.0), 400, false);
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.fill_text(if d.get("error").is_some() { "暂无数据" } else { "加载中…" }, inner.rect.left, inner.rect.top);
        return;
    };
    c.set_baseline(Baseline::Top);
    c.set_align(Align::Left);
    c.set_fill(t.fg);
    c.set_font((inner.rect.height() * 0.2).clamp(12.0, 22.0), 500, false);
    let repo = d.get("repo").and_then(Value::as_str).unwrap_or_else(|| cfg.get("repo").and_then(Value::as_str).unwrap_or(""));
    let repo = c.ellipsis(repo, inner.rect.width());
    c.fill_text(&repo, inner.rect.left, inner.rect.top);
    c.set_fill(t.accent);
    c.set_font((inner.rect.height() * 0.3).clamp(13.0, 34.0), 300, true);
    c.fill_text(&format!("★ {stars}"), inner.rect.left, inner.rect.top + inner.rect.height() * 0.3);
    c.set_fill(t.fg4);
    c.set_font((inner.rect.height() * 0.13).max(10.0), 400, true);
    let lang = d.get("lang").and_then(Value::as_str).unwrap_or("");
    let issues = d.get("issues").and_then(Value::as_i64).unwrap_or(0);
    c.fill_text(&format!("{lang} · issue {issues}"), inner.rect.left, inner.rect.top + inner.rect.height() * 0.78);
}
