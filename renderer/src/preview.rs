use std::sync::{Arc, Mutex};
use std::thread;

use tiny_http::{Header, Response, Server};

pub type PreviewFrame = Arc<Mutex<Vec<u8>>>;

pub fn start(addr: &str) -> Option<PreviewFrame> {
    let server = match Server::http(addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("preview server {addr}: {e}");
            return None;
        }
    };
    let frame: PreviewFrame = Arc::new(Mutex::new(Vec::new()));
    let shared = frame.clone();
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let path = request.url().split('?').next().unwrap_or("/");
            match path {
                "/preview.png" => {
                    let data = shared.lock().map(|v| v.clone()).unwrap_or_default();
                    let header = Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..])
                        .expect("header");
                    let _ = request.respond(Response::from_data(data).with_header(header));
                }
                "/health" => {
                    let body = if shared.lock().map(|v| !v.is_empty()).unwrap_or(false) {
                        r#"{"ok":true,"frame":"ready"}"#
                    } else {
                        r#"{"ok":true,"frame":"pending"}"#
                    };
                    let _ = request.respond(Response::from_string(body));
                }
                _ => {
                    let _ = request.respond(Response::from_string("not found").with_status_code(404));
                }
            }
        }
    });
    Some(frame)
}
