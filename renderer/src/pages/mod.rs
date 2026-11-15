pub mod stock;

use serde_json::Value;
use skia_safe::Rect;

use crate::canvas::Canvas2D;
use crate::data::Need;
use crate::layout::Page as LayoutPage;
use crate::theme::Theme;

pub fn rust_page(page: &LayoutPage) -> Option<&'static dyn RustPage> {
    let module = page.module.as_deref().unwrap_or("");
    let page_name = page.page.as_deref().unwrap_or("");
    if page.kind == "rust" || page.kind == "custom" {
        if page_name == "stock" || module.ends_with("stock.js") || page.id == "stockpage" {
            return Some(&stock::STOCK);
        }
    }
    None
}

pub trait RustPage {
    fn needs(&self, cfg: &Value) -> Vec<Need>;
    fn draw(&self, c: &mut Canvas2D, size: (f32, f32), data: &Value, cfg: &Value, t: &Theme);
}

pub fn needs(page: &LayoutPage) -> Vec<Need> {
    rust_page(page).map(|p| p.needs(&page.config)).unwrap_or_default()
}

pub fn draw(page: &LayoutPage, c: &mut Canvas2D, data: &Value, w: f32, h: f32, t: &Theme) {
    match rust_page(page) {
        Some(p) => p.draw(c, (w, h), data, &page.config, t),
        None => {
            c.set_fill(t.bg);
            c.fill_rect(Rect::from_xywh(0.0, 0.0, w, h));
            c.set_fill(t.fg4);
            c.set_font(24.0, 400, false);
            c.set_align(crate::canvas::Align::Center);
            c.set_baseline(crate::canvas::Baseline::Middle);
            c.fill_text("Rust page not found", w / 2.0, h / 2.0);
        }
    }
}
