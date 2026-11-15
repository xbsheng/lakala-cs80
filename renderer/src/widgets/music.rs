use std::time::Duration;

use serde_json::Value;
use skia_safe::Rect;

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;
use crate::widgets::common::card;

pub fn needs(_cfg: &Value) -> Vec<Need> {
    vec![Need::new("music", vec![], Duration::from_secs(5))]
}

pub fn draw(c: &mut Canvas2D, r: Rect, cfg: &Value, d: &Value, t: &Theme) {
    let pad = (r.width() * 0.06).max(10.0);
    let playing = d.get("playing").and_then(Value::as_bool).unwrap_or(false);
    let title = cfg.get("title").and_then(Value::as_str).unwrap_or("正在播放");
    let meta = if playing { d.get("app").and_then(Value::as_str).unwrap_or("") } else { "" };
    let inner = card(c, r, t, title, meta, t.fg4, pad);
    c.set_baseline(Baseline::Top);
    c.set_align(Align::Left);
    c.set_fill(if playing { t.fg } else { t.fg4 });
    c.set_font((inner.rect.height() * 0.22).clamp(12.0, 24.0), 500, false);
    let title = if playing {
        d.get("title").and_then(Value::as_str).unwrap_or("")
    } else if d.get("error").is_some() {
        "需要授权"
    } else {
        "未在播放"
    };
    let title = c.ellipsis(title, inner.rect.width());
    c.fill_text(&title, inner.rect.left, inner.rect.top + inner.rect.height() * 0.14);
    c.set_fill(t.fg4);
    c.set_font((inner.rect.height() * 0.14).max(10.0), 400, false);
    let artist = if playing { d.get("artist").and_then(Value::as_str).unwrap_or("") } else { "" };
    let artist = c.ellipsis(artist, inner.rect.width());
    c.fill_text(&artist, inner.rect.left, inner.rect.top + inner.rect.height() * 0.62);
    if playing {
        c.set_fill(t.accent);
        c.set_font((inner.rect.height() * 0.18).max(12.0), 400, false);
        c.set_align(Align::Right);
        c.fill_text("♫", inner.rect.right, inner.rect.top + inner.rect.height() * 0.6);
    }
}
