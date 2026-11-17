// 顶部消息弹窗（iOS 通知横幅样式）：Agent 任务结束时从屏幕上方落下，
// 停留 5 s 后缓慢淡出。叠在任何页面之上，屏保播放时暂停（随推帧一起停）。
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use serde_json::Value;
use skia_safe::{Color, Rect};

use crate::assets::Assets;
use crate::canvas::{Align, Baseline, Canvas2D};
use crate::data::Need;
use crate::theme::Theme;

const CARD_W: f32 = 736.0;
const CARD_H: f32 = 166.0;
const CARD_X: f32 = 32.0;
const CARD_Y: f32 = 26.0;
const RADIUS: f32 = 44.0;
const PAD: f32 = 26.0;
const PAD_V: f32 = 24.0;
const ICON: f32 = 40.0;
const IN_MS: f32 = 380.0;
const HOLD_MS: f32 = 5000.0;
const OUT_MS: f32 = 560.0;
const MAX_QUEUE: usize = 3;

pub fn need() -> Need {
    Need::new("agents", vec![], Duration::from_secs(3))
}

struct Banner {
    agent: String,
    app: String,
    title: String,
    body: String,
}

pub struct Notifier {
    /// 会话 key → (上次状态, 已运行秒数)
    seen: HashMap<String, (String, Option<i64>)>,
    cur: Option<(Banner, Instant)>,
    queue: VecDeque<Banner>,
}

impl Notifier {
    pub fn new() -> Self {
        Self {
            seen: HashMap::new(),
            cur: None,
            queue: VecDeque::new(),
        }
    }

    /// 调试用（CS80_NOTIFY_DEMO=1）：启动就来一条，方便截图看动效。
    pub fn demo() -> Self {
        let mut n = Self::new();
        n.queue.push_back(Banner {
            agent: "pi".into(),
            app: "PI".into(),
            title: "lakala-cs80 · 整理一下文件代码".into(),
            body: "任务完成 · 用时 3m".into(),
        });
        n
    }

    pub fn visible(&self) -> bool {
        self.cur.is_some() || !self.queue.is_empty()
    }

    /// 每帧喂一次 `/api/data/agents`；只对「运行中/等待确认 → 完成/异常」的跳变出弹窗。
    pub fn poll(&mut self, d: &Value) {
        let Some(items) = d.get("items").and_then(Value::as_array) else {
            return;
        };
        let mut next: HashMap<String, (String, Option<i64>)> = HashMap::new();
        for it in items {
            let id = it.get("id").and_then(Value::as_str).unwrap_or("");
            if id.is_empty() {
                continue;
            }
            let agent = it.get("agent").and_then(Value::as_str).unwrap_or("");
            let state = it.get("state").and_then(Value::as_str).unwrap_or("");
            let running_for = it.get("runningFor").and_then(Value::as_i64);
            let key = format!("{agent}/{id}");
            if let Some((prev, prev_for)) = self.seen.get(&key) {
                let finished = matches!(state, "done" | "error");
                let was_busy = matches!(prev.as_str(), "running" | "waiting");
                if finished && was_busy {
                    self.enqueue(it, state, running_for.or(*prev_for));
                }
            }
            next.insert(key, (state.to_string(), running_for));
        }
        self.seen = next;
    }

    fn enqueue(&mut self, it: &Value, state: &str, running_for: Option<i64>) {
        if self.queue.len() >= MAX_QUEUE {
            return;
        }
        let name = it.get("name").and_then(Value::as_str).unwrap_or("Agent");
        let body = match state {
            "error" => "本轮异常结束".to_string(),
            _ => match running_for {
                Some(secs) if secs >= 1 => format!("任务完成 · 用时 {}", dur_txt(secs)),
                _ => "任务完成".to_string(),
            },
        };
        self.queue.push_back(Banner {
            agent: it.get("agent").and_then(Value::as_str).unwrap_or("").to_string(),
            app: name.to_uppercase(),
            title: it
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .or_else(|| it.get("cwd").and_then(Value::as_str))
                .unwrap_or(name)
                .to_string(),
            body,
        });
    }

    /// 画当前这条；队列空了什么都不画。
    pub fn draw(&mut self, c: &mut Canvas2D, t: &Theme, assets: &mut Assets) {
        loop {
            match &self.cur {
                Some((_, at)) if at.elapsed().as_secs_f32() * 1000.0 > IN_MS + HOLD_MS + OUT_MS => {
                    self.cur = None;
                }
                Some(_) => break,
                None => match self.queue.pop_front() {
                    Some(b) => self.cur = Some((b, Instant::now())),
                    None => return,
                },
            }
        }
        let Some((b, at)) = &self.cur else { return };
        let ms = at.elapsed().as_secs_f32() * 1000.0;
        let (y, alpha) = if ms < IN_MS {
            let p = ms / IN_MS;
            let e = ease_out_back(p);
            (-CARD_H - 40.0 + (CARD_Y + CARD_H + 40.0) * e, (p / 0.35).min(1.0))
        } else if ms < IN_MS + HOLD_MS {
            (CARD_Y, 1.0)
        } else {
            let p = ((ms - IN_MS - HOLD_MS) / OUT_MS).clamp(0.0, 1.0);
            let e = ease_in_out(p);
            (CARD_Y - 44.0 * e, 1.0 - e)
        };
        if alpha <= 0.01 {
            return;
        }

        c.save();
        c.set_alpha(alpha);
        let card = Rect::from_xywh(CARD_X, y, CARD_W, CARD_H);
        // 没有 blur，用几层错位圆角矩形当阴影
        for (grow, a) in [(12.0f32, 0x0a), (6.0, 0x10), (2.0, 0x14)] {
            c.set_fill(Color::from_argb(a, 0x00, 0x00, 0x00));
            c.fill_rrect(
                Rect::from_xywh(
                    CARD_X - grow,
                    y + 7.0 - grow,
                    CARD_W + grow * 2.0,
                    CARD_H + grow * 2.0,
                ),
                RADIUS + grow,
            );
        }
        c.set_fill(Color::from_argb(0xfa, 0x25, 0x27, 0x2f));
        c.fill_rrect(card, RADIUS);
        c.set_stroke(Color::from_argb(0x1c, 0xff, 0xff, 0xff));
        c.set_line_width(1.0);
        c.stroke_rrect(card, RADIUS);

        // 第一行：App 图标 + App 名 + 右侧时间
        let icon = ICON;
        let ix = CARD_X + PAD;
        let cy = y + PAD_V + icon / 2.0;
        if !assets.draw_logo(c, &b.agent, ix, cy - icon / 2.0, icon, icon) {
            c.set_fill(Color::from_argb(0x2e, 0xff, 0xff, 0xff));
            c.fill_rrect(Rect::from_xywh(ix, cy - icon / 2.0, icon, icon), 12.0);
            c.set_fill(t.fg);
            c.set_font(icon * 0.5, 700, false);
            c.set_align(Align::Center);
            c.set_baseline(Baseline::Middle);
            let letter = b.app.chars().next().unwrap_or('A').to_string();
            c.fill_text(&letter, ix + icon / 2.0, cy + 1.0);
        }
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Middle);
        c.set_fill(t.fg3);
        c.set_font(21.0, 600, false);
        let app = c.ellipsis(&b.app, CARD_W * 0.5);
        c.tracked_text(&app, ix + icon + 14.0, cy + 1.0, 1.6);
        c.set_align(Align::Right);
        c.set_fill(t.fg4);
        c.set_font(20.0, 400, false);
        c.fill_text("刚刚", CARD_X + CARD_W - PAD, cy + 1.0);

        // 标题 + 正文
        let text_w = CARD_W - PAD * 2.0;
        c.set_align(Align::Left);
        c.set_baseline(Baseline::Top);
        c.set_fill(t.fg);
        c.set_font(26.0, 600, false);
        let title = c.ellipsis(&b.title, text_w);
        c.fill_text(&title, CARD_X + PAD, y + PAD_V + ICON + 16.0);
        c.set_fill(t.fg3);
        c.set_font(23.0, 400, false);
        c.fill_text(&b.body, CARD_X + PAD, y + PAD_V + ICON + 52.0);
        c.restore();
    }
}

fn dur_txt(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", (secs as f64 / 60.0).round() as i64)
    } else {
        format!("{}h", (secs as f64 / 3600.0).round() as i64)
    }
}

/// iOS 弹性弹出：末端轻微回弹。
fn ease_out_back(p: f32) -> f32 {
    let s = 1.3;
    let q = p - 1.0;
    1.0 + (s + 1.0) * q * q * q + s * q * q
}

fn ease_in_out(p: f32) -> f32 {
    if p < 0.5 {
        2.0 * p * p
    } else {
        1.0 - (-2.0 * p + 2.0).powi(2) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Fonts;
    use serde_json::json;

    fn agents(state: &str, running_for: Option<i64>) -> Value {
        json!({"items": [{
            "id": "abc", "agent": "pi", "name": "Pi", "state": state,
            "title": "lakala-cs80 · 提交代码", "cwd": "lakala-cs80",
            "runningFor": running_for
        }]})
    }

    #[test]
    fn notifies_once_on_finish() {
        let mut n = Notifier::new();
        n.poll(&agents("running", Some(150)));
        assert!(!n.visible(), "首次观测到运行中不应该弹");
        n.poll(&agents("done", None));
        assert!(n.visible(), "运行 → 完成要弹一条");
        assert_eq!(n.queue.len(), 1);
        assert_eq!(n.queue[0].body, "任务完成 · 用时 3m");
        n.poll(&agents("done", None));
        assert_eq!(n.queue.len(), 1, "done 重复上报不能重复弹");
    }

    #[test]
    fn error_and_waiting() {
        let mut n = Notifier::new();
        n.poll(&agents("waiting", None));
        n.poll(&agents("error", None));
        assert_eq!(n.queue[0].body, "本轮异常结束");
        // waiting → done 也算一轮结束
        let mut n = Notifier::new();
        n.poll(&agents("waiting", None));
        n.poll(&agents("done", None));
        assert_eq!(n.queue.len(), 1);
    }

    #[test]
    fn ignores_idle_restart() {
        let mut n = Notifier::new();
        n.poll(&agents("done", None)); // 冷启动看到的历史 done
        assert!(!n.visible());
        n.poll(&Value::Null);
        n.poll(&json!({"loading": true}));
        assert!(!n.visible());
    }
    #[test]
    fn banner_is_opaque_while_held() {
        // 停留阶段必须盖住底下的页面内容（白底验证），否则会像半透明蒙版。
        let fonts = Fonts::new();
        let theme = Theme::default();
        let mut assets = Assets::new();
        let mut surface = skia_safe::surfaces::raster_n32_premul((800, 1280)).expect("surface");
        surface.canvas().clear(Color::WHITE);
        let mut n = Notifier::new();
        n.cur = Some((
            Banner {
                agent: "pi".into(),
                app: "PI".into(),
                title: "标题".into(),
                body: "正文".into(),
            },
            Instant::now() - Duration::from_millis(2000),
        ));
        let mut c = Canvas2D::new(surface.canvas(), &fonts);
        n.draw(&mut c, &theme, &mut assets);
        let img = surface.image_snapshot();
        let pm = img.peek_pixels().expect("pixmap");
        let bytes = pm.bytes().expect("bytes");
        let p = 60 * pm.row_bytes() + 400 * 4;
        let (r, g, b) = (bytes[p], bytes[p + 1], bytes[p + 2]);
        assert!(r < 60 && g < 60 && b < 60, "卡片不透明度不够: {r},{g},{b}");
    }
}
