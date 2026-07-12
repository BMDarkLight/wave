//! Tiny loopback HTTP server that exposes the current track's cover art.
//!
//! `tauri-plugin-media-session` downloads notification artwork itself via
//! `HttpURLConnection` — it cannot read `file://` URIs or embedded `data:`
//! URLs. To get embedded (ID3 / Vorbis comment) cover art onto the Android
//! lock screen / notification, we decode it in Rust and serve the bytes over
//! `http://127.0.0.1:<port>/...` so the plugin's native downloader can fetch
//! them like it would any other image URL.
//!
//! The server only binds to loopback and gates requests behind a random
//! per-run token embedded in the URL path, so other apps on the device can't
//! usefully guess the artwork URL.

#![cfg(target_os = "android")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

struct ServerHandle {
    port: u16,
    token: String,
    state: Arc<Mutex<Option<(Vec<u8>, String)>>>,
}

static HANDLE: OnceLock<Option<ServerHandle>> = OnceLock::new();
static VERSION: AtomicU32 = AtomicU32::new(0);

fn start() -> Option<ServerHandle> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let port = listener.local_addr().ok()?.port();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let state: Arc<Mutex<Option<(Vec<u8>, String)>>> = Arc::new(Mutex::new(None));

    let thread_state = Arc::clone(&state);
    let thread_token = token.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let state = Arc::clone(&thread_state);
            let token = thread_token.clone();
            std::thread::spawn(move || handle_connection(stream, &state, &token));
        }
    });

    tracing::info!("Local cover-art server listening on 127.0.0.1:{port}");
    Some(ServerHandle { port, token, state })
}

fn handle_connection(mut stream: TcpStream, state: &Arc<Mutex<Option<(Vec<u8>, String)>>>, token: &str) {
    let mut buf = [0u8; 1024];
    let mut request = Vec::new();
    loop {
        let Ok(n) = stream.read(&mut buf) else { return };
        if n == 0 {
            break;
        }
        request.extend_from_slice(&buf[..n]);
        if request.windows(4).any(|w| w == b"\r\n\r\n") || request.len() > 8192 {
            break;
        }
    }

    let request_line = request.split(|&b| b == b'\n').next().unwrap_or(&[]);
    let request_str = String::from_utf8_lossy(request_line);
    let authorized = request_str.contains(token);

    let payload = if authorized {
        state.lock().ok().and_then(|guard| guard.as_ref().cloned())
    } else {
        None
    };

    let _ = match payload {
        Some((bytes, content_type)) => {
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                bytes.len()
            );
            stream
                .write_all(header.as_bytes())
                .and_then(|_| stream.write_all(&bytes))
        }
        None => stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    };
}

fn handle() -> Option<&'static ServerHandle> {
    HANDLE.get_or_init(start).as_ref()
}

/// Publish new artwork bytes and return the loopback URL to hand to the
/// media-session plugin. Returns `None` if the server failed to start.
pub fn publish(bytes: Vec<u8>, content_type: &str, ext: &str) -> Option<String> {
    let handle = handle()?;
    if let Ok(mut guard) = handle.state.lock() {
        *guard = Some((bytes, content_type.to_string()));
    }
    let version = VERSION.fetch_add(1, Ordering::Relaxed) + 1;
    Some(format!(
        "http://127.0.0.1:{}/{}/cover-{version}.{ext}",
        handle.port, handle.token
    ))
}
