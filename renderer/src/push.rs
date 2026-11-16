use anyhow::Result;
use serde_json::Value;
use skia_safe::{ColorType, Surface};

pub const WIDTH: usize = 800;
pub const HEIGHT: usize = 1280;
const TILE_W: usize = 32;
const TILE_H: usize = 16;

pub struct Push {
    host: String,
    agent: ureq::Agent,
    prev: Option<Vec<u16>>,
}

impl Push {
    pub fn new(host: impl Into<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(3))
            .build();
        Self {
            host: host.into().trim_end_matches('/').to_string(),
            agent,
            prev: None,
        }
    }

    pub fn reset(&mut self) {
        self.prev = None;
    }

    /// 返回 true 表示 host 已接收（入队）。
    pub fn push(&mut self, surface: &mut Surface) -> bool {
        let curr = bgr565(surface);
        if self.prev.is_none() {
            return self.full(surface, &curr);
        }
        let prev = self.prev.as_ref().unwrap();
        let mut dirty = Vec::new();
        for ty in 0..(HEIGHT / TILE_H) {
            for tx in 0..(WIDTH / TILE_W) {
                let mut changed = false;
                'rows: for row in 0..TILE_H {
                    let y = ty * TILE_H + row;
                    let base = y * WIDTH + tx * TILE_W;
                    if prev[base..base + TILE_W] != curr[base..base + TILE_W] {
                        changed = true;
                        break 'rows;
                    }
                }
                if changed {
                    dirty.push((tx, ty));
                }
            }
        }
        if dirty.is_empty() {
            self.prev = Some(curr);
            return true;
        }
        if dirty.len() > 120 {
            return self.full(surface, &curr);
        }

        let mut body = Vec::with_capacity(4 + dirty.len() * (4 + TILE_W * TILE_H * 2));
        body.extend_from_slice(&(dirty.len() as u32).to_le_bytes());
        for (tx, ty) in dirty {
            body.extend_from_slice(&((tx * TILE_W) as u16).to_le_bytes());
            body.extend_from_slice(&((ty * TILE_H) as u16).to_le_bytes());
            for row in 0..TILE_H {
                let y = ty * TILE_H + row;
                let base = y * WIDTH + tx * TILE_W;
                for px in &curr[base..base + TILE_W] {
                    body.extend_from_slice(&px.to_le_bytes());
                }
            }
        }
        match self.post("/api/patch", &body) {
            Ok(true) => {
                self.prev = Some(curr);
                true
            }
            Ok(false) => {
                self.prev = None;
                false
            }
            Err(e) => {
                eprintln!("push patch: {e}");
                self.prev = None;
                false
            }
        }
    }

    fn full(&mut self, surface: &mut Surface, curr: &[u16]) -> bool {
        let rgb = rgb888(surface);
        match self.post("/api/frame", &rgb) {
            Ok(true) => {
                self.prev = Some(curr.to_vec());
                true
            }
            Ok(false) => {
                self.prev = None;
                false
            }
            Err(e) => {
                eprintln!("push full: {e}");
                self.prev = None;
                false
            }
        }
    }

    fn post(&self, path: &str, body: &[u8]) -> Result<bool> {
        let resp = self
            .agent
            .post(&format!("{}{}", self.host, path))
            .set("Content-Type", "application/octet-stream")
            .send_bytes(body)?;
        let v: Value = resp.into_json()?;
        let ok = v.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if !ok {
            eprintln!("push {path} rejected: {v}");
        }
        Ok(ok)
    }
}

fn rgba_bytes<'a>(surface: &'a mut Surface) -> (Vec<u8>, bool, usize) {
    let image = surface.image_snapshot();
    let pixmap = image.peek_pixels().expect("pixmap");
    let info = pixmap.info();
    let bytes = pixmap.bytes().expect("pixels").to_vec();
    let bgra = matches!(info.color_type(), ColorType::BGRA8888);
    (bytes, bgra, info.width() as usize)
}

fn bgr565(surface: &mut Surface) -> Vec<u16> {
    let (bytes, bgra, width) = rgba_bytes(surface);
    let mut out = vec![0u16; WIDTH * HEIGHT];
    for i in 0..WIDTH * HEIGHT {
        let p = i * 4;
        let (r, g, b) = if bgra {
            (bytes[p + 2], bytes[p + 1], bytes[p])
        } else {
            (bytes[p], bytes[p + 1], bytes[p + 2])
        };
        out[i] = (((b as u16) & 0xf8) << 8) | (((g as u16) & 0xfc) << 3) | ((r as u16) >> 3);
    }
    let _ = width;
    out
}

fn rgb888(surface: &mut Surface) -> Vec<u8> {
    let (bytes, bgra, _) = rgba_bytes(surface);
    let mut out = vec![0u8; WIDTH * HEIGHT * 3];
    for i in 0..WIDTH * HEIGHT {
        let p = i * 4;
        let q = i * 3;
        if bgra {
            out[q] = bytes[p + 2];
            out[q + 1] = bytes[p + 1];
            out[q + 2] = bytes[p];
        } else {
            out[q] = bytes[p];
            out[q + 1] = bytes[p + 1];
            out[q + 2] = bytes[p + 2];
        }
    }
    out
}
