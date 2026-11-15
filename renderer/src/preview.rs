use std::sync::{Arc, Mutex};
use std::thread;

use tiny_http::{Header, Method, Response, Server};

pub type PreviewFrame = Arc<Mutex<Vec<u8>>>;
pub type PreviewPage = Arc<Mutex<Option<String>>>;

pub struct Preview {
    pub frame: PreviewFrame,
    pub page: PreviewPage,
}

fn cors() -> Vec<Header> {
    vec![
        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
        Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET, POST, OPTIONS"[..]).unwrap(),
        Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..]).unwrap(),
    ]
}

fn query(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    for part in q.split('&') {
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        if k == key {
            return Some(urlencoding::decode(v).map(|s| s.into_owned()).unwrap_or_else(|_| v.to_string()));
        }
    }
    None
}

pub fn start(addr: &str) -> Option<Preview> {
    let server = match Server::http(addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("preview server {addr}: {e}");
            return None;
        }
    };
    let frame: PreviewFrame = Arc::new(Mutex::new(Vec::new()));
    let page: PreviewPage = Arc::new(Mutex::new(None));
    let shared = frame.clone();
    let requested = page.clone();
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let url = request.url().to_string();
            let path = url.split('?').next().unwrap_or("/");
            if request.method() == &Method::Options {
                let mut resp = Response::empty(204);
                for h in cors() { resp.add_header(h); }
                let _ = request.respond(resp);
                continue;
            }
            match path {
                "/preview.png" => {
                    let data = shared.lock().map(|v| v.clone()).unwrap_or_default();
                    let mut resp = Response::from_data(data);
                    if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..]) {
                        resp.add_header(h);
                    }
                    for h in cors() { resp.add_header(h); }
                    let _ = request.respond(resp);
                }
                "/preview/page" => {
                    let id = query(&url, "page").filter(|s| !s.is_empty());
                    if let Ok(mut g) = requested.lock() {
                        *g = id.clone();
                    }
                    let body = serde_json::json!({"ok": true, "page": id}).to_string();
                    let mut resp = Response::from_string(body);
                    if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
                        resp.add_header(h);
                    }
                    for h in cors() { resp.add_header(h); }
                    let _ = request.respond(resp);
                }
                "/health" => {
                    let ready = shared.lock().map(|v| !v.is_empty()).unwrap_or(false);
                    let body = format!("{{\"ok\":true,\"frame\":\"{}\"}}", if ready { "ready" } else { "pending" });
                    let mut resp = Response::from_string(body);
                    for h in cors() { resp.add_header(h); }
                    let _ = request.respond(resp);
                }
                _ => {
                    let mut resp = Response::from_string("not found").with_status_code(404);
                    for h in cors() { resp.add_header(h); }
                    let _ = request.respond(resp);
                }
            }
        }
    });
    Some(Preview { frame, page })
}
