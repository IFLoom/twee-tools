//! `twee-tools dev`: build, serve, and on every change build again and reload the browser. A build that fails keeps the
//! last good page served, with the errors over it, so a mistake never stops the session.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::Watcher;

use crate::pipeline::{self, Outcome, Settings};
use crate::render;

/// What the server shows: the last good page, and the errors of the last build if it failed.
#[derive(Default)]
struct State {
    page: Option<String>,
    failure: Option<String>,
}

/// The browser's end: reload on a good build, show the errors on a failed one. Added before `</body>`.
const CLIENT: &str = r#"<script id="twee-tools-dev">(function () {
  var events = new EventSource('/__twee-tools/events');
  events.addEventListener('reload', function () { location.reload(); });
  events.addEventListener('failed', function (e) { show(JSON.parse(e.data)); });
  function show(text) {
    var box = document.getElementById('twee-tools-overlay');
    if (!box) {
      box = document.createElement('div');
      box.id = 'twee-tools-overlay';
      box.style.cssText = 'position:fixed;inset:0;z-index:2147483647;background:rgba(12,12,16,.94);color:#f3f3f3;overflow:auto;padding:24px 28px;font:13px/1.5 ui-monospace,Menlo,Consolas,monospace';
      document.documentElement.appendChild(box);
    }
    box.innerHTML = '<div style="font:600 15px system-ui,sans-serif;color:#ff8a80;margin-bottom:12px">twee-tools: the story didn\'t build. The page below is the last build that did.</div><pre style="white-space:pre-wrap;margin:0"></pre>';
    box.querySelector('pre').textContent = text;
  }
  if (window.__tweeToolsFailure) { show(window.__tweeToolsFailure); }
})();</script>"#;

fn with_client(html: &str, failure: Option<&str>) -> String {
    let failure =
        failure.map(|f| format!("<script>window.__tweeToolsFailure = {};</script>", serde_json::to_string(f).unwrap_or_default())).unwrap_or_default();
    let client = format!("{failure}{CLIENT}");
    match html.rfind("</body>") {
        Some(at) => format!("{}{client}{}", &html[..at], &html[at..]),
        None => format!("{html}{client}"),
    }
}

/// The page to send for `/`: the last good build (with the errors over it if the newest failed), or the errors alone.
fn current_page(state: &State) -> String {
    match (&state.page, &state.failure) {
        (Some(page), failure) => with_client(page, failure.as_deref()),
        (None, Some(failure)) => {
            with_client("<!doctype html><html><head><meta charset=\"utf-8\"><title>twee-tools</title></head><body></body></html>", Some(failure))
        }
        (None, None) => with_client("<!doctype html><html><head><meta charset=\"utf-8\"><title>twee-tools</title></head><body>Building…</body></html>", None),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("svg") => "image/svg+xml",
        Some("mp3") => "audio/mpeg",
        Some("ogg" | "oga") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("mp4" | "m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn decode_path(raw: &str) -> String {
    let path = raw.split(['?', '#']).next().unwrap_or("");
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            let hex = |b: u8| (b as char).to_digit(16).unwrap_or(0) as u8;
            out.push(hex(bytes[i + 1]) * 16 + hex(bytes[i + 2]));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Serves requests: the page, the events, and files (media beside the page, or in the project).
fn serve(server: tiny_http::Server, state: Arc<Mutex<State>>, clients: Arc<Mutex<Vec<Sender<String>>>>, roots: Vec<PathBuf>) {
    for request in server.incoming_requests() {
        let url = decode_path(request.url());
        let header = |name: &str, value: &str| tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap();
        if url == "/__twee-tools/events" {
            // Written straight to the socket and flushed after each event: a buffered response would hold them back.
            let (to, from) = channel::<String>();
            clients.lock().unwrap().push(to);
            let mut socket = request.into_writer();
            std::thread::spawn(move || {
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n: connected\n\n";
                if socket.write_all(head.as_bytes()).and_then(|_| socket.flush()).is_err() {
                    return;
                }
                loop {
                    let message = match from.recv_timeout(Duration::from_secs(15)) {
                        Ok(message) => message,
                        // A comment line now and then keeps the connection open.
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => ": still here\n\n".into(),
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                    };
                    if socket.write_all(message.as_bytes()).and_then(|_| socket.flush()).is_err() {
                        return;
                    }
                }
            });
            continue;
        }
        if url == "/" || url == "/index.html" {
            let page = current_page(&state.lock().unwrap());
            let response = tiny_http::Response::from_string(page)
                .with_header(header("Content-Type", "text/html; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store"));
            let _ = request.respond(response);
            continue;
        }
        let relative = url.trim_start_matches('/');
        let safe = !relative.split('/').any(|part| part == "..");
        let file = safe.then(|| roots.iter().map(|root| root.join(relative)).find(|p| p.is_file())).flatten();
        let response = match file.and_then(|f| std::fs::read(&f).ok().map(|bytes| (f, bytes))) {
            Some((path, bytes)) => tiny_http::Response::from_data(bytes).with_header(header("Content-Type", content_type(&path))),
            None => tiny_http::Response::from_data(b"Not found".to_vec()).with_status_code(404),
        };
        let _ = request.respond(response);
    }
}

fn broadcast(clients: &Mutex<Vec<Sender<String>>>, event: &str, data: &str) {
    let message = format!("event: {event}\ndata: {}\n\n", serde_json::to_string(data).unwrap_or_default());
    clients.lock().unwrap().retain(|client| client.send(message.clone()).is_ok());
}

/// One build: writes the page, updates what's served, tells the browsers, and says what happened in the terminal.
fn rebuild(settings: &Settings, state: &Mutex<State>, clients: &Mutex<Vec<Sender<String>>>, color: crate::Color, first: bool) {
    let started = Instant::now();
    let outcome = pipeline::run(settings, &|_| {});
    let plain = render::Style::new(false);
    let width = 100;
    let failure = match outcome {
        Outcome::Built { text, story, report, passages, format, .. } => {
            if let Some(report) = &report {
                crate::show_report(&story, report, color);
            }
            let written = pipeline::write(&settings.output, &text).and_then(|_| pipeline::copy_assets(&settings.copied(), &settings.output).map(|_| ()));
            let mut state = state.lock().unwrap();
            state.page = Some(text);
            state.failure = None;
            drop(state);
            if let Err(error) = written {
                eprintln!("{error}");
            }
            eprintln!("✔ Built: {passages} passages, {format}, in {:.1}s", started.elapsed().as_secs_f64());
            if !first {
                broadcast(clients, "reload", "");
            }
            return;
        }
        Outcome::Stopped { story, report } => {
            crate::show_report(&story, &report, color);
            eprintln!(" Not built: fix the errors and save; the last good build is still served.\n");
            render::report(&story, &report, &plain, &render::Options { all: true, width })
        }
        // dev never asks for a check alone.
        Outcome::Checked { story, report } => render::report(&story, &report, &plain, &render::Options { all: true, width }),
        Outcome::Failed(error) => {
            eprintln!("{error}\n Not built: fix it and save; the last good build is still served.\n");
            error
        }
    };
    state.lock().unwrap().failure = Some(failure.clone());
    broadcast(clients, "failed", &failure);
}

/// Opens a URL in the browser, as well as the system allows.
fn open(url: &str) {
    let tries: &[(&str, &[&str])] = &[("wslview", &[]), ("xdg-open", &[]), ("open", &[]), ("cmd.exe", &["/c", "start"])];
    for (program, before) in tries {
        let mut command = std::process::Command::new(program);
        command.args(*before).arg(url).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        if command.status().is_ok_and(|s| s.success()) {
            return;
        }
    }
}

pub fn run(settings: Settings, color: crate::Color) -> ExitCode {
    let settings = match Settings::new(settings) {
        Ok(s) => s,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let (port, open_browser) = (settings.port.unwrap_or(8080), settings.open);
    // A port that's free, from the one asked for up.
    let (server, port) = match (port..port.saturating_add(20)).find_map(|p| tiny_http::Server::http(("127.0.0.1", p)).ok().map(|s| (s, p))) {
        Some(found) => found,
        None => {
            eprintln!("No port free from {port} to {}.", port.saturating_add(19));
            return ExitCode::from(1);
        }
    };
    let state = Arc::new(Mutex::new(State::default()));
    let clients: Arc<Mutex<Vec<Sender<String>>>> = Arc::new(Mutex::new(vec![]));
    // Files are looked for beside the page, then in the working folder, then beside each source.
    let mut roots: Vec<PathBuf> = vec![];
    roots.extend(settings.output.parent().map(Path::to_path_buf));
    roots.push(PathBuf::from("."));
    for path in &settings.paths {
        roots.push(if path.is_dir() { path.clone() } else { path.parent().map(Path::to_path_buf).unwrap_or_default() });
    }
    {
        let (state, clients) = (state.clone(), clients.clone());
        std::thread::spawn(move || serve(server, state, clients, roots));
    }
    let url = format!("http://127.0.0.1:{port}/");
    eprintln!("twee-tools dev: {url}  (Ctrl+C to stop)");
    rebuild(&settings, &state, &clients, color, true);
    if open_browser {
        open(&url);
    }

    // Polled rather than told: changes made from Windows to a WSL path (or over a network) are seen too.
    let (to, from) = channel();
    let config = notify::Config::default().with_poll_interval(Duration::from_millis(400));
    let mut watcher = match notify::PollWatcher::new(to, config) {
        Ok(w) => w,
        Err(error) => {
            eprintln!("Can't watch for changes: {error}");
            return ExitCode::from(1);
        }
    };
    let output = std::fs::canonicalize(&settings.output).unwrap_or_else(|_| settings.output.clone());
    for path in settings.watched() {
        let mode = if path.is_dir() { notify::RecursiveMode::Recursive } else { notify::RecursiveMode::NonRecursive };
        if let Err(error) = watcher.watch(&path, mode) {
            eprintln!("Can't watch {}: {error}", path.display());
        }
    }
    while let Ok(first) = from.recv() {
        // Several files saved at once are one change: wait a moment and take the rest.
        let mut events = vec![first];
        while let Ok(more) = from.recv_timeout(Duration::from_millis(150)) {
            events.push(more);
        }
        let relevant = events
            .iter()
            .filter_map(|e| e.as_ref().ok())
            .flat_map(|e| e.paths.iter())
            .any(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()) != output);
        if relevant {
            let changed: Vec<String> = events
                .iter()
                .filter_map(|e| e.as_ref().ok())
                .flat_map(|e| e.paths.iter())
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            eprintln!("Changed: {}", changed.join(", "));
            rebuild(&settings, &state, &clients, color, false);
        }
    }
    ExitCode::SUCCESS
}
