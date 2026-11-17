use skia_safe::{
    gradient::{Colors, Gradient, Interpolation},
    paint::{Cap, Join},
    shaders, Canvas, Color, Data, Font, FontMgr, FontStyle, Image, Paint, PaintStyle, PathBuilder,
    PathEffect, Point, RRect, Rect, TileMode, Typeface,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Baseline {
    Top,
    Middle,
    Bottom,
    Alphabetic,
}

pub struct Fonts {
    sans: Typeface,
    mono: Typeface,
}

impl Fonts {
    pub fn new() -> Self {
        let mgr = FontMgr::new();
        // 先试 Linux 设备上的字体文件，再按家族名找系统字体（macOS CoreText / Linux fontconfig）。
        let sans = load_face(
            &mgr,
            &[
                "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
                "/System/Library/Fonts/PingFang.ttc",
                "/System/Library/Fonts/Hiragino Sans GB.ttc",
            ],
            &["WenQuanYi Zen Hei", "PingFang SC", "Hiragino Sans GB"],
        );
        let mono = load_face(
            &mgr,
            &[
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
                "/System/Library/Fonts/Menlo.ttc",
                "/System/Library/Fonts/SFNSMono.ttf",
            ],
            &["DejaVu Sans Mono", "Menlo", "SF Mono"],
        );
        Self { sans, mono }
    }

    pub fn font(&self, size: f32, weight: u16, mono: bool) -> Font {
        let tf = if mono { self.mono.clone() } else { self.sans.clone() };
        let mut f = Font::new(tf, size);
        f.set_subpixel(true);
        f.set_embolden(weight >= 600);
        f
    }
}

fn load_face(mgr: &FontMgr, paths: &[&str], families: &[&str]) -> Typeface {
    paths
        .iter()
        .find_map(|p| Data::from_filename(p).and_then(|d| mgr.new_from_data(d, 0)))
        .or_else(|| {
            families
                .iter()
                .find_map(|f| mgr.match_family_style(f, FontStyle::normal()))
        })
        .or_else(|| mgr.legacy_make_typeface(None, FontStyle::normal()))
        .expect("no system typeface available")
}

#[derive(Clone)]
struct State {
    fill: Color,
    stroke: Color,
    line_width: f32,
    cap: Cap,
    join: Join,
    dash: Vec<f32>,
    alpha: f32,
    font_size: f32,
    font_weight: u16,
    mono: bool,
    align: Align,
    baseline: Baseline,
}

pub struct Canvas2D<'a> {
    canvas: &'a Canvas,
    fonts: &'a Fonts,
    fill: Color,
    stroke: Color,
    line_width: f32,
    cap: Cap,
    join: Join,
    dash: Vec<f32>,
    alpha: f32,
    font_size: f32,
    font_weight: u16,
    mono: bool,
    font: Font,
    align: Align,
    baseline: Baseline,
    stack: Vec<State>,
}

impl<'a> Canvas2D<'a> {
    pub fn new(canvas: &'a Canvas, fonts: &'a Fonts) -> Self {
        let font = fonts.font(16.0, 400, false);
        Self {
            canvas,
            fonts,
            fill: Color::BLACK,
            stroke: Color::BLACK,
            line_width: 1.0,
            cap: Cap::Butt,
            join: Join::Miter,
            dash: Vec::new(),
            alpha: 1.0,
            font_size: 16.0,
            font_weight: 400,
            mono: false,
            font,
            align: Align::Left,
            baseline: Baseline::Alphabetic,
            stack: Vec::new(),
        }
    }

    fn state(&self) -> State {
        State {
            fill: self.fill,
            stroke: self.stroke,
            line_width: self.line_width,
            cap: self.cap,
            join: self.join,
            dash: self.dash.clone(),
            alpha: self.alpha,
            font_size: self.font_size,
            font_weight: self.font_weight,
            mono: self.mono,
            align: self.align,
            baseline: self.baseline,
        }
    }

    pub fn save(&mut self) {
        self.stack.push(self.state());
        self.canvas.save();
    }

    pub fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.fill = s.fill;
            self.stroke = s.stroke;
            self.line_width = s.line_width;
            self.cap = s.cap;
            self.join = s.join;
            self.dash = s.dash;
            self.alpha = s.alpha;
            self.font_size = s.font_size;
            self.font_weight = s.font_weight;
            self.mono = s.mono;
            self.align = s.align;
            self.baseline = s.baseline;
            self.font = self.fonts.font(self.font_size, self.font_weight, self.mono);
        }
        self.canvas.restore();
    }

    pub fn set_fill(&mut self, c: Color) {
        self.fill = c;
    }

    pub fn set_stroke(&mut self, c: Color) {
        self.stroke = c;
    }

    pub fn set_line_width(&mut self, w: f32) {
        self.line_width = w;
    }

    pub fn set_line_cap_round(&mut self) {
        self.cap = Cap::Round;
    }

    pub fn set_line_join_round(&mut self) {
        self.join = Join::Round;
    }

    pub fn set_dash(&mut self, dash: &[f32]) {
        self.dash = dash.to_vec();
    }

    pub fn set_alpha(&mut self, a: f32) {
        self.alpha = a;
    }

    pub fn set_font(&mut self, size: f32, weight: u16, mono: bool) {
        self.font_size = size;
        self.font_weight = weight;
        self.mono = mono;
        self.font = self.fonts.font(size, weight, mono);
    }

    pub fn set_align(&mut self, a: Align) {
        self.align = a;
    }

    pub fn set_baseline(&mut self, b: Baseline) {
        self.baseline = b;
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.canvas.translate((dx, dy));
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.canvas.scale((sx, sy));
    }

    fn with_alpha(&self, c: Color) -> Color {
        let a = (c.a() as f32 * self.alpha).round().clamp(0.0, 255.0) as u8;
        Color::from_argb(a, c.r(), c.g(), c.b())
    }

    fn fill_paint(&self) -> Paint {
        let mut p = Paint::default();
        p.set_anti_alias(true);
        p.set_color(self.with_alpha(self.fill));
        p
    }

    fn stroke_paint(&self) -> Paint {
        let mut p = Paint::default();
        p.set_anti_alias(true);
        p.set_style(PaintStyle::Stroke);
        p.set_color(self.with_alpha(self.stroke));
        p.set_stroke_width(self.line_width);
        p.set_stroke_cap(self.cap);
        p.set_stroke_join(self.join);
        if !self.dash.is_empty() {
            if let Some(effect) = PathEffect::dash(&self.dash, 0.0) {
                p.set_path_effect(effect);
            }
        }
        p
    }

    pub fn fill_rect(&mut self, r: Rect) {
        self.canvas.draw_rect(r, &self.fill_paint());
    }

    pub fn fill_rrect(&mut self, r: Rect, radius: f32) {
        self.canvas
            .draw_rrect(RRect::new_rect_xy(r, radius, radius), &self.fill_paint());
    }

    pub fn stroke_rrect(&mut self, r: Rect, radius: f32) {
        self.canvas
            .draw_rrect(RRect::new_rect_xy(r, radius, radius), &self.stroke_paint());
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.canvas
            .draw_line((x1, y1), (x2, y2), &self.stroke_paint());
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32) {
        self.canvas.draw_circle((cx, cy), r, &self.fill_paint());
    }

    pub fn stroke_arc(&mut self, cx: f32, cy: f32, r: f32, start_deg: f32, sweep_deg: f32) {
        let oval = Rect::from_xywh(cx - r, cy - r, r * 2.0, r * 2.0);
        self.canvas
            .draw_arc(oval, start_deg, sweep_deg, false, &self.stroke_paint());
    }

    pub fn stroke_polyline(&mut self, pts: &[(f32, f32)]) {
        if pts.len() < 2 {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0]);
        for p in &pts[1..] {
            pb.line_to(*p);
        }
        let path = pb.detach();
        self.canvas.draw_path(&path, &self.stroke_paint());
    }

    /// 折线下方填充：从 baseline_y 到曲线，颜色从 top_color 渐隐到底部透明。
    pub fn fill_area_gradient(
        &mut self,
        pts: &[(f32, f32)],
        baseline_y: f32,
        top_color: Color,
        bottom_color: Color,
        top_y: f32,
        bottom_y: f32,
    ) {
        if pts.len() < 2 {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0]);
        for p in &pts[1..] {
            pb.line_to(*p);
        }
        pb.line_to((pts[pts.len() - 1].0, baseline_y));
        pb.line_to((pts[0].0, baseline_y));
        pb.close();
        let path = pb.detach();
        let colors = [top_color.into(), bottom_color.into()];
        let gc = Colors::new_evenly_spaced(&colors, TileMode::Clamp, None);
        let grad = Gradient::new(gc, Interpolation::default());
        let shader = shaders::linear_gradient(
            (Point::new(0.0, top_y), Point::new(0.0, bottom_y)),
            &grad,
            None,
        );
        let mut p = Paint::default();
        p.set_anti_alias(true);
        if let Some(shader) = shader {
            p.set_shader(shader);
        } else {
            p.set_color(top_color);
        }
        self.canvas.draw_path(&path, &p);
    }

    pub fn draw_image(&mut self, img: &Image, x: f32, y: f32, w: f32, h: f32) {
        let mut p = Paint::default();
        p.set_anti_alias(true);
        p.set_alpha_f(self.alpha);
        self.canvas
            .draw_image_rect(img, None, Rect::from_xywh(x, y, w, h), &p);
    }

    pub fn fill_polygon(&mut self, pts: &[(f32, f32)]) {
        if pts.len() < 2 {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0]);
        for p in &pts[1..] {
            pb.line_to(*p);
        }
        pb.close();
        let path = pb.detach();
        self.canvas.draw_path(&path, &self.fill_paint());
    }

    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32) {
        self.canvas.draw_circle((cx, cy), r, &self.stroke_paint());
    }

    fn font_for(&self, text: &str) -> Font {
        if self.mono && text.chars().any(|c| !c.is_ascii()) {
            self.fonts.font(self.font_size, self.font_weight, false)
        } else {
            self.fonts.font(self.font_size, self.font_weight, self.mono)
        }
    }

    pub fn measure_text(&self, text: &str) -> f32 {
        let paint = self.fill_paint();
        self.font_for(text).measure_text(text, Some(&paint)).0
    }

    fn baseline_y_for(&self, font: &Font, y: f32) -> f32 {
        let (_, m) = font.metrics();
        match self.baseline {
            Baseline::Top => y - m.ascent,
            Baseline::Middle => y - (m.ascent + m.descent) / 2.0,
            Baseline::Bottom => y - m.descent,
            Baseline::Alphabetic => y,
        }
    }

    pub fn fill_text(&mut self, text: &str, x: f32, y: f32) {
        let font = self.font_for(text);
        let w = {
            let paint = self.fill_paint();
            font.measure_text(text, Some(&paint)).0
        };
        let x = match self.align {
            Align::Left => x,
            Align::Center => x - w / 2.0,
            Align::Right => x - w,
        };
        let y = self.baseline_y_for(&font, y);
        self.canvas
            .draw_str(text, (x, y), &font, &self.fill_paint());
    }

    /// Canvas 2D 没有 letter-spacing，这里逐字画。
    pub fn tracked_text(&mut self, text: &str, x: f32, y: f32, spacing: f32) {
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return;
        }
        let widths: Vec<f32> = chars
            .iter()
            .map(|c| self.measure_text(&c.to_string()))
            .collect();
        let total: f32 = widths.iter().sum::<f32>() + spacing * (chars.len() as f32 - 1.0);
        let mut cx = match self.align {
            Align::Left => x,
            Align::Center => x - total / 2.0,
            Align::Right => x - total,
        };
        for (ch, w) in chars.iter().zip(widths.iter()) {
            let text = ch.to_string();
            let font = self.font_for(&text);
            let by = self.baseline_y_for(&font, y);
            self.canvas
                .draw_str(&text, (cx, by), &font, &self.fill_paint());
            cx += *w + spacing;
        }
    }

    pub fn ellipsis(&mut self, text: &str, max_w: f32) -> String {
        if self.measure_text(text) <= max_w {
            return text.to_string();
        }
        let mut s: Vec<char> = text.chars().collect();
        while s.len() > 1 {
            s.pop();
            let candidate = format!("{}…", s.iter().collect::<String>());
            if self.measure_text(&candidate) <= max_w {
                return candidate;
            }
        }
        "…".to_string()
    }

    pub fn fit_font(&mut self, text: &str, max_w: f32, size: f32, weight: u16, mono: bool) -> f32 {
        let mut s = size;
        for _ in 0..26 {
            self.set_font(s, weight, mono);
            if self.measure_text(text) <= max_w || s <= 9.0 {
                break;
            }
            s *= 0.92;
        }
        s
    }

    pub fn canvas(&self) -> &Canvas {
        self.canvas
    }
}
