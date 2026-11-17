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
    /// 连续失败次数 / 下次重试时间：没接屏时别每帧都转 3 MB 整帧 + 刷日志。
    fail_streak: u32,
    retry_at: Option<std::time::Instant>,
    // 复用缓冲：每帧 4 MB 像素 + 2 MB BGR565 若重复分配，macOS malloc 不归还导致 RSS 翻倍。
    scratch: Vec<u16>,
    rgba: Vec<u8>,
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
            fail_streak: 0,
            retry_at: None,
            scratch: Vec::new(),
            rgba: Vec::new(),
        }
    }

    /// 退避中 = 这一帧无论渲染得多漂亮也发不出去（没接屏 / 设备掉线）。
    pub fn backing_off(&self) -> bool {
        self.retry_at.map(|at| std::time::Instant::now() < at).unwrap_or(false)
    }

    pub fn reset(&mut self) {
        self.prev = None;
        self.fail_streak = 0;
        self.retry_at = None;
    }

    /// 返回 true 表示 host 已接收（入队）。
    pub fn push(&mut self, surface: &mut Surface) -> bool {
        if let Some(at) = self.retry_at {
            if std::time::Instant::now() < at {
                return false; // 退避中：连像素转换都省了
            }
            self.retry_at = None;
        }
        let mut curr = std::mem::take(&mut self.scratch);
        curr.resize(WIDTH * HEIGHT, 0);
        self.fill_bgr565(surface, &mut curr);

        let Some(prev) = self.prev.take() else {
            let ok = self.full(surface, &curr);
            if ok { self.prev = Some(curr); } else { self.scratch = curr; }
            return ok;
        };

        let dirty = diff(&prev, &curr);
        if dirty.is_empty() {
            self.prev = Some(curr);
            self.scratch = prev;
            return true;
        }
        if dirty.len() > 120 {
            let ok = self.full(surface, &curr);
            if ok {
                self.prev = Some(curr);
                self.scratch = prev;
            } else {
                self.scratch = curr;
            }
            return ok;
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
                self.ok_reset();
                self.prev = Some(curr);
                self.scratch = prev;
                true
            }
            Ok(false) => {
                self.reject("patch");
                self.prev = None;
                self.scratch = curr;
                false
            }
            Err(e) => {
                self.fail(e, "patch");
                self.prev = None;
                self.scratch = curr;
                false
            }
        }
    }

    fn full(&mut self, surface: &mut Surface, curr: &[u16]) -> bool {
        let rgb = self.rgb888(surface);
        match self.post("/api/frame", &rgb) {
            Ok(true) => {
                self.ok_reset();
                self.prev = Some(curr.to_vec());
                true
            }
            Ok(false) => {
                self.reject("full");
                self.prev = None;
                false
            }
            Err(e) => {
                self.fail(e, "full");
                self.prev = None;
                false
            }
        }
    }

    /// 连续失败按 0.5→1→2→4→8 s 退避；日志只在第 1 次、每 50 次和恢复时打，别把 journal 刷爆。
    fn note_fail(&mut self, desc: String) {
        self.fail_streak += 1;
        if self.fail_streak == 1 || self.fail_streak % 50 == 0 {
            eprintln!("{}（连续 {} 次失败，退避中）", desc, self.fail_streak);
        }
        let secs = 0.5 * 2f32.powi(self.fail_streak.min(5) as i32 - 1);
        self.retry_at =
            Some(std::time::Instant::now() + std::time::Duration::from_secs_f32(secs.min(8.0)));
    }

    fn fail(&mut self, e: anyhow::Error, what: &str) {
        self.note_fail(format!("push {what}: {e}"));
    }

    /// host 回了 ok=false：设备没接 / 掉线 / 固件拒收。
    fn reject(&mut self, what: &str) {
        self.note_fail(format!("push {what}: host 拒绝（设备未连接？）"));
    }

    fn ok_reset(&mut self) {
        if self.fail_streak > 0 {
            eprintln!("push 恢复（之前连续失败 {} 次）", self.fail_streak);
        }
        self.fail_streak = 0;
        self.retry_at = None;
    }

    fn post(&self, path: &str, body: &[u8]) -> Result<bool> {
        let resp = self
            .agent
            .post(&format!("{}{}", self.host, path))
            .set("Content-Type", "application/octet-stream")
            .send_bytes(body)?;
        let v: Value = resp.into_json()?;
        Ok(v.get("ok").and_then(Value::as_bool).unwrap_or(false))
    }

    fn fill_rgba(&mut self, surface: &mut Surface) -> (bool, usize) {
        let image = surface.image_snapshot();
        let pixmap = image.peek_pixels().expect("pixmap");
        let info = pixmap.info();
        let bytes = pixmap.bytes().expect("pixels");
        self.rgba.clear();
        self.rgba.extend_from_slice(bytes);
        let bgra = matches!(info.color_type(), ColorType::BGRA8888);
        (bgra, info.width() as usize)
    }

    fn fill_bgr565(&mut self, surface: &mut Surface, out: &mut [u16]) {
        let (bgra, _width) = self.fill_rgba(surface);
        let bytes = &self.rgba;
        for (i, px) in out.iter_mut().enumerate() {
            let p = i * 4;
            let (r, g, b) = if bgra {
                (bytes[p + 2], bytes[p + 1], bytes[p])
            } else {
                (bytes[p], bytes[p + 1], bytes[p + 2])
            };
            *px = (((b as u16) & 0xf8) << 8) | (((g as u16) & 0xfc) << 3) | ((r as u16) >> 3);
        }
    }

    fn rgb888(&mut self, surface: &mut Surface) -> Vec<u8> {
        let (bgra, _) = self.fill_rgba(surface);
        let bytes = &self.rgba;
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
}

fn diff(prev: &[u16], curr: &[u16]) -> Vec<(usize, usize)> {
    let mut dirty = Vec::new();
    for ty in 0..(HEIGHT / TILE_H) {
        for tx in 0..(WIDTH / TILE_W) {
            for row in 0..TILE_H {
                let y = ty * TILE_H + row;
                let base = y * WIDTH + tx * TILE_W;
                if prev[base..base + TILE_W] != curr[base..base + TILE_W] {
                    dirty.push((tx, ty));
                    break;
                }
            }
        }
    }
    dirty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_detects_changed_tile_only() {
        let prev = vec![0u16; WIDTH * HEIGHT];
        let mut curr = prev.clone();
        curr[0] = 1; // 左上角第一个 tile
        assert_eq!(diff(&prev, &curr), vec![(0, 0)]);
        assert!(diff(&prev, &prev).is_empty());
        curr[5 * TILE_H * WIDTH + 5 * TILE_W] = 1;
        assert_eq!(diff(&prev, &curr), vec![(0, 0), (5, 5)]);
    }
}
