mod assets;
mod canvas;
mod data;
mod layout;
mod lunar;
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
    let mut fps = 1.0f32;

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
            "--push" => { push_enabled = true; i += 1; }
            "--once" => { once = true; i += 1; }
            other => { eprintln!("unknown arg: {other}"); i += 1; }
        }
    }

    let store = DataStore::new(&host);
    let mut data = DataStore::new(&host);
    let watch_layout = layout_path.is_none();
    let mut layout = load_layout(&store, layout_path.as_ref())?;
    let fonts = Fonts::new();
    let theme = Theme::default();
    let mut assets = Assets::new();

    if once {
        let page = layout.active_page();
        let mut surface = match page {
            Some(p) => render_page(p, &mut data, &fonts, &theme, &mut assets),
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
    let mut last_saver_poll = Instant::now();
    let mut saver_playing = false;
    let frame_time = Duration::from_secs_f32(1.0 / fps);
    println!("renderer: layout={} fps={fps} push={push_enabled}", layout.active_page().map(|p| p.id.as_str()).unwrap_or("-"));

    loop {
        let started = Instant::now();
        if watch_layout && last_poll.elapsed() >= Duration::from_secs(2) {
            last_poll = Instant::now();
            if let Ok(v) = store.layout() {
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

        if last_saver_poll.elapsed() >= Duration::from_secs(2) {
            last_saver_poll = Instant::now();
            if let Ok(v) = store.screensaver() {
                let was = saver_playing;
                saver_playing = v.get("saver").and_then(|s| s.get("playing")).and_then(Value::as_bool).unwrap_or(false);
                if was && !saver_playing {
                    if let Some(p) = &mut pusher { p.reset(); }
                }
            }
        }

        let active_id = layout.active_page().map(|p| p.id.clone()).unwrap_or_default();
        let surface = match layout.active_page() {
            Some(page) => render_page(page, &mut data, &fonts, &theme, &mut assets),
            None => empty_surface(&theme),
        };
        let mut surface = surface;
        if let Some(f) = &preview_frame {
            let png = encode_png(&mut surface);
            if let Ok(mut g) = f.lock() { *g = png; }
        }

        // 编辑器可以请求预览某个指定页面（不影响设备推送的 active page）。
        if let (Some(f), Some(req)) = (&preview_frame, &preview_page) {
            let requested = req.lock().ok().and_then(|g| g.clone());
            if let Some(id) = requested {
                if id != active_id {
                    if let Some(page) = layout.pages.iter().find(|p| p.id == id) {
                        let mut pv = render_page(page, &mut data, &fonts, &theme, &mut assets);
                        let png = encode_png(&mut pv);
                        if let Ok(mut g) = f.lock() { *g = png; }
                    }
                }
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

fn render_page(page: &LayoutPage, data: &mut DataStore, fonts: &Fonts, theme: &Theme, assets: &mut Assets) -> Surface {
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

fn encode_png(surface: &mut Surface) -> Vec<u8> {
    let image = surface.image_snapshot();
    image
        .encode_to_data(EncodedImageFormat::PNG)
        .map(|d| d.as_bytes().to_vec())
        .unwrap_or_default()
}
