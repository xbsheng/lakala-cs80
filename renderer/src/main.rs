mod assets;
mod canvas;
mod data;
mod layout;
mod lunar;
mod notify;
mod pages;
mod preview;
mod push;
mod theme;
mod widgets;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::Value;
use skia_safe::{surfaces, EncodedImageFormat, Surface};

use crate::assets::Assets;
use crate::canvas::{Canvas2D, Fonts};
use crate::data::DataStore;
use crate::layout::{Layout, Page as LayoutPage};
use crate::push::Push;
use crate::theme::Theme;

fn main() -> Result<()> {
    let mut host = "http://127.0.0.1:8787".to_string();
    let mut layout_path: Option<PathBuf> = None;
    let mut out = "/tmp/cs80-renderer.png".to_string();
    let mut push_enabled = false;
    let mut once = false;
    let mut preview_port = 8788u16;
    let mut preview_bind = "0.0.0.0".to_string();
    let mut fps = 5.0f32;
    let mut notify_sound: Option<String> = Some(format!(
        "{}/../docs/assets/audio/计时器结束提示音.mp3",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut notify_volume = 20u8;

    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--host" if i + 1 < args.len() => { host = args[i + 1].clone(); i += 2; }
            "--layout" if i + 1 < args.len() => { layout_path = Some(PathBuf::from(&args[i + 1])); i += 2; }
            "--out" if i + 1 < args.len() => { out = args[i + 1].clone(); i += 2; }
            "--preview-port" if i + 1 < args.len() => { preview_port = args[i + 1].parse().unwrap_or(8788); i += 2; }
            "--preview-bind" if i + 1 < args.len() => { preview_bind = args[i + 1].clone(); i += 2; }
            "--fps" if i + 1 < args.len() => { fps = args[i + 1].parse::<f32>().unwrap_or(1.0).max(0.2); i += 2; }
            "--notify-sound" if i + 1 < args.len() => {
                let v = args[i + 1].clone();
                notify_sound = if v == "off" || v.is_empty() { None } else { Some(v) };
                i += 2;
            }
            "--notify-volume" if i + 1 < args.len() => {
                notify_volume = args[i + 1].parse::<u8>().unwrap_or(20).min(100);
                i += 2;
            }
            "--push" => { push_enabled = true; i += 1; }
            "--once" => { once = true; i += 1; }
            other => { eprintln!("unknown arg: {other}"); i += 1; }
        }
    }

    let data = DataStore::new(&host);
    let watch_layout = layout_path.is_none();
    let mut layout = load_layout(&data, layout_path.as_ref())?;
    let fonts = Fonts::new();
    let theme = Theme::default();
    let mut assets = Assets::new();
    // 顶部消息弹窗；CS80_NOTIFY_DEMO=1 启动就来一条，方便看动效。
    let mut notifier = notify::Notifier::new(&host, notify_sound.and_then(|p| {
        if std::path::Path::new(&p).exists() {
            Some((
                fs::canonicalize(&p).map(|c| c.display().to_string()).unwrap_or(p),
                notify_volume,
            ))
        } else {
            eprintln!("notify sound: 文件不存在 -> {p}（用 --notify-sound off 关掉）");
            None
        }
    }));
    if env::var_os("CS80_NOTIFY_DEMO").is_some() {
        notifier.push_demo();
    }

    if once {
        let page = layout.active_page();
        let mut surface = match page {
            Some(p) => render_page(p, &data, &fonts, &theme, &mut assets),
            None => empty_surface(&theme),
        };
        let png = encode_png(&mut surface);
        fs::write(&out, &png)?;
        println!("wrote {out} ({} bytes)", png.len());
        if push_enabled {
            let mut p = Push::new(&host);
            let ok = p.push(&mut surface);
            println!("push: {ok}");
        }
        return Ok(());
    }

    let preview = if preview_port > 0 {
        let addr = format!("{preview_bind}:{preview_port}");
        let p = preview::start(&addr);
        if p.is_some() {
            println!("preview: http://{addr}/preview.png");
        }
        p
    } else {
        None
    };
    let preview_frame = preview.as_ref().map(|p| p.frame.clone());
    let preview_page = preview.as_ref().map(|p| p.page.clone());

    let mut pusher = if push_enabled {
        println!("push: enabled -> {host}");
        Some(Push::new(&host))
    } else {
        None
    };

    let mut last_layout = serde_json::to_string(&layout)?;
    let mut last_poll = Instant::now();
    let mut saver_playing = false;
    let mut last_preview_at = Instant::now() - Duration::from_secs(1);
    let mut last_preview_page_at = Instant::now() - Duration::from_secs(1);
    let mut last_preview_page_id: Option<String> = None;
    println!("renderer: layout={} fps={fps} push={push_enabled}", layout.active_page().map(|p| p.id.as_str()).unwrap_or("-"));

    loop {
        let started = Instant::now();
        let max_fps = fps.max(0.2);
        // 没人看预览、推帧又在退避 → 10 Hz 渲染纯属空转，降到 1 Hz。
        let preview_live = preview
            .as_ref()
            .map(|p| {
                let recent = p.viewers.lock().map(|at| at.elapsed().as_secs() < 10).unwrap_or(false);
                // 帧还没生成过（刚启动/刚有人第一个请求）就得先编一张，否则第一次拉是空 body。
                let empty = p.frame.lock().map(|f| f.is_empty()).unwrap_or(false);
                recent || empty
            })
            .unwrap_or(false);
        let idle = !preview_live
            && pusher.as_ref().map(|p| p.backing_off()).unwrap_or(true)
            && !notifier.visible();
        let frame_time = if idle {
            Duration::from_secs(1)
        } else if page_animated(&layout) || notifier.visible() {
            Duration::from_secs_f32(1.0 / max_fps)
        } else {
            Duration::from_secs(1)
        };
        if watch_layout && last_poll.elapsed() >= Duration::from_secs(2) {
            last_poll = Instant::now();
            if let Some(v) = data.layout_snapshot() {
                let s = serde_json::to_string(&v)?;
                if s != last_layout {
                    match serde_json::from_value::<Layout>(v) {
                        Ok(next) => {
                            println!("layout changed -> {}", next.active_page().map(|p| p.id.as_str()).unwrap_or("-"));
                            last_layout = s;
                            layout = next;
                            if let Some(p) = &mut pusher { p.reset(); }
                        }
                        Err(e) => eprintln!("layout parse: {e}"),
                    }
                }
            }
        }

        let playing_now = data.screensaver_playing();
        if saver_playing && !playing_now {
            if let Some(p) = &mut pusher { p.reset(); }
        }
        saver_playing = playing_now;

        let active_id = layout.active_page().map(|p| p.id.clone()).unwrap_or_default();
        let surface = match layout.active_page() {
            Some(page) => render_page(page, &data, &fonts, &theme, &mut assets),
            None => empty_surface(&theme),
        };
        let mut surface = surface;
        // 通知横幅叠在页面之上（屏保期间不画，随推帧一起停）。
        if !saver_playing {
            notifier.poll(&data.get(&notify::need()));
            if notifier.visible() {
                let mut c = Canvas2D::new(surface.canvas(), &fonts);
                notifier.draw(&mut c, &theme, &mut assets);
            }
        }
        // 没人拉 /preview.png（编辑器没开）就不必每秒编码一张 800×1280 PNG。
        if let (Some(f), true) = (&preview_frame, preview_live) {
            // 弹窗动画期间预览跟着 10 Hz 刷新，否则编辑器里看到的是跳帧。
            let preview_every = if notifier.visible() {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(1)
            };
            if last_preview_at.elapsed() >= preview_every {
                last_preview_at = Instant::now();
                let png = encode_png(&mut surface);
                if let Ok(mut g) = f.lock() { *g = png; }
            }
        }

        // 编辑器可以请求预览某个指定页面（不影响设备推送的 active page）。
        if let (Some(f), Some(req)) = (&preview_frame, &preview_page) {
            let requested = req.lock().ok().and_then(|g| g.clone());
            // 编辑器每隔 2.5s 续租；编辑器关掉/切走后 6s 自动停止预览页渲染，
            // 避免 renderer 一直额外拉 stockpage 的数据。
            let requested = requested.filter(|(_, at)| at.elapsed() < Duration::from_secs(6));
            let changed = requested.as_ref().map(|(id, _)| id) != last_preview_page_id.as_ref();
            if let Some((id, _)) = requested {
                if id != active_id
                    && (changed || last_preview_page_at.elapsed() >= Duration::from_secs(1))
                {
                    last_preview_page_at = Instant::now();
                    last_preview_page_id = Some(id.clone());
                    if let Some(page) = layout.pages.iter().find(|p| p.id == id) {
                        let mut pv = render_page(page, &data, &fonts, &theme, &mut assets);
                        let png = encode_png(&mut pv);
                        if let Ok(mut g) = f.lock() { *g = png; }
                    }
                }
            } else {
                last_preview_page_id = None;
            }
        }

        if let Some(p) = &mut pusher {
            if !saver_playing {
                p.push(&mut surface);
            }
        }
        thread::sleep(frame_time.saturating_sub(started.elapsed()));
    }
}

fn page_animated(layout: &Layout) -> bool {
    layout
        .active_page()
        .map(|p| {
            p.widgets.iter().any(|w| {
                matches!(
                    w.kind.as_str(),
                    "clock" | "clock_dial" | "runcat" | "runcat_bar" | "agent"
                )
            })
        })
        .unwrap_or(false)
}

fn load_layout(store: &DataStore, path: Option<&PathBuf>) -> Result<Layout> {
    let value = if let Some(path) = path {
        serde_json::from_str::<Value>(&fs::read_to_string(path)?)?
    } else {
        match store.layout() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("layout from host failed ({e}); fallback to ~/.cs80_panel/layout.json");
                let home = env::var("HOME").unwrap_or_else(|_| "/root".into());
                serde_json::from_str::<Value>(&fs::read_to_string(format!("{home}/.cs80_panel/layout.json"))?)?
            }
        }
    };
    Ok(serde_json::from_value(value)?)
}

fn empty_surface(theme: &Theme) -> Surface {
    let mut surface = surfaces::raster_n32_premul((800, 1280)).expect("surface");
    surface.canvas().clear(theme.bg);
    surface
}

fn render_page(page: &LayoutPage, data: &DataStore, fonts: &Fonts, theme: &Theme, assets: &mut Assets) -> Surface {
    let mut surface = surfaces::raster_n32_premul((800, 1280)).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(theme.bg);
    let mut c = Canvas2D::new(canvas, fonts);
    if page.kind == "grid" {
        for w in &page.widgets {
            let mut value = Value::Null;
            for need in widgets::needs(w) {
                value = data.get(&need);
            }
            widgets::draw(w, &mut c, &value, theme, assets);
        }
    } else {
        let mut value = Value::Null;
        for need in pages::needs(page) {
            value = data.get(&need);
        }
        pages::draw(page, &mut c, &value, 800.0, 1280.0, theme);
    }
    surface
}

// raster surface 走 encode_to_data 没有替代 API（新 API 针对 GPU 上下文），保持弃用但可用。
#[allow(deprecated)]
fn encode_png(surface: &mut Surface) -> Vec<u8> {
    let image = surface.image_snapshot();
    image
        .encode_to_data(EncodedImageFormat::PNG)
        .map(|d| d.as_bytes().to_vec())
        .unwrap_or_default()
}
