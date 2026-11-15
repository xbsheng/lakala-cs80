use skia_safe::Rect;

use crate::canvas::{Align, Baseline, Canvas2D};
use crate::theme::Theme;
use crate::widgets::common::card;

pub fn draw(c: &mut Canvas2D, id: &str, kind: &str, r: Rect, t: &Theme) {
    let inner = card(
        c,
        r,
        t,
        &format!("P1 · {kind}"),
        id,
        t.fg4,
        (r.width() * 0.03).max(10.0),
    );
    c.set_align(Align::Center);
    c.set_baseline(Baseline::Middle);
    c.set_font((inner.rect.height() * 0.16).clamp(14.0, 34.0), 500, false);
    c.set_fill(t.fg4);
    c.fill_text(
        "Rust renderer · widget pending",
        inner.rect.left + inner.rect.width() / 2.0,
        inner.rect.top + inner.rect.height() / 2.0,
    );
}
