use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use skia_safe::{svg::Dom, surfaces, BlendMode, Color, Data, FontMgr, Image, Paint, Rect};

use crate::canvas::Canvas2D;

const CAT_W: f32 = 120.0;
const CAT_H: f32 = 100.0;
const CAT_AR: f32 = CAT_H / CAT_W;
const CAT_CONTENT_CY: f32 = 0.635;
const HIST_N: usize = 60;

pub struct CatState {
    frame: usize,
    at: Instant,
    acc: f32,
}

impl Default for CatState {
    fn default() -> Self {
        Self {
            frame: 0,
            at: Instant::now(),
            acc: 0.0,
        }
    }
}

impl CatState {
    pub fn frame(&mut self, cpu: f32) -> usize {
        let now = Instant::now();
        if now.duration_since(self.at) >= Duration::from_millis(150) {
            self.at = now;
            self.acc += 1.0 + 2.0 * cpu.clamp(0.0, 1.0);
            let step = self.acc.floor().clamp(1.0, 2.0) as usize;
            self.acc -= step as f32;
            self.frame = (self.frame + step) % 5;
        }
        self.frame
    }
}

pub struct History {
    pub cpu: Vec<f32>,
    pub mem: Vec<f32>,
    at: Instant,
}

impl Default for History {
    fn default() -> Self {
        Self {
            cpu: Vec::new(),
            mem: Vec::new(),
            at: Instant::now(),
        }
    }
}

impl History {
    pub fn sample(&mut self, cpu: f32, mem: f32) {
        if self.cpu.is_empty() {
            self.cpu = vec![cpu; HIST_N];
            self.mem = vec![mem; HIST_N];
            self.at = Instant::now();
            return;
        }
        if self.at.elapsed() < Duration::from_millis(900) {
            return;
        }
        self.at = Instant::now();
        self.cpu.push(cpu);
        self.mem.push(mem);
        if self.cpu.len() > HIST_N {
            self.cpu.remove(0);
            self.mem.remove(0);
        }
    }
}

pub struct Assets {
    cat: Vec<Image>,
    cat_tinted: HashMap<(usize, u32), Image>,
    logos: HashMap<String, Dom>,
    pub cat_state: CatState,
    pub history: History,
}

impl Assets {
    pub fn new() -> Self {
        let web = find_web();
        let mut cat = Vec::new();
        for i in 0..5 {
            let path = web.join(format!("assets/runcat/cat_running_{i}.png"));
            if let Some(img) = Data::from_filename(&path).and_then(Image::from_encoded) {
                cat.push(img);
            }
        }
        let mut logos = HashMap::new();
        for (key, file) in [
            ("pi", "logos/pi.svg"),
            ("claude", "logos/claude.svg"),
            ("codex", "logos/openai.svg"),
            ("deepseek", "logos/deepseek.svg"),
        ] {
            let path = web.join(file);
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(dom) = Dom::from_bytes(&bytes, FontMgr::default()) {
                    logos.insert(key.to_string(), dom);
                }
            }
        }
        Self {
            cat,
            cat_tinted: HashMap::new(),
            logos,
            cat_state: CatState::default(),
            history: History::default(),
        }
    }

    pub fn cat_frame(&mut self, cpu: f32) -> usize {
        self.cat_state.frame(cpu)
    }

    pub fn sample(&mut self, cpu: f32, mem: f32) {
        self.history.sample(cpu, mem);
    }

    pub fn draw_cat(&mut self, c: &mut Canvas2D, cx: f32, cy: f32, w: f32, color: Color, frame: usize, feet: bool) -> bool {
        if self.cat.is_empty() {
            return false;
        }
        let frame = frame % self.cat.len();
        let key = (frame, color_key(color));
        let img = if let Some(img) = self.cat_tinted.get(&key) {
            img.clone()
        } else {
            let Some(src) = self.cat.get(frame) else {
                return false;
            };
            let mut surface = match surfaces::raster_n32_premul((CAT_W as i32, CAT_H as i32)) {
                Some(s) => s,
                None => return false,
            };
            let canvas = surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            canvas.draw_image_rect(src, None, Rect::from_xywh(0.0, 0.0, CAT_W, CAT_H), &Paint::default());
            let mut p = Paint::default();
            p.set_color(color);
            p.set_blend_mode(BlendMode::SrcIn);
            canvas.draw_rect(Rect::from_xywh(0.0, 0.0, CAT_W, CAT_H), &p);
            let img = surface.image_snapshot();
            self.cat_tinted.insert(key, img.clone());
            img
        };
        let h = w * CAT_AR;
        let top = if feet { cy - h } else { cy - h * CAT_CONTENT_CY };
        c.draw_image(&img, cx - w / 2.0, top, w, h);
        true
    }

    pub fn draw_logo(&self, c: &mut Canvas2D, agent: &str, x: f32, y: f32, w: f32, h: f32) -> bool {
        let Some(dom) = self.logos.get(agent) else {
            return false;
        };
        let size = dom.root().intrinsic_size();
        let sx = w / size.width.max(1.0);
        let sy = h / size.height.max(1.0);
        c.save();
        c.translate(x, y);
        c.scale(sx, sy);
        dom.render(c.canvas());
        c.restore();
        true
    }
}

fn color_key(c: Color) -> u32 {
    ((c.a() as u32) << 24) | ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32
}

fn find_web() -> PathBuf {
    if let Ok(p) = std::env::var("CS80_WEB") {
        return PathBuf::from(p);
    }
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../panel/web"),
        PathBuf::from("panel/web"),
        PathBuf::from("web"),
    ];
    for p in candidates {
        if p.join("widgets.js").exists() {
            return p;
        }
    }
    PathBuf::from("../panel/web")
}
