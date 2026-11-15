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
