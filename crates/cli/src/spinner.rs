//! A spinner on stderr while a job runs, with what it's doing: "⠹ Checking media files: 1200 of 2570". Only when stderr is a
//! terminal, so piped output, --json and redirected reports never see it. It clears its line when it stops.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct Spinner {
    message: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl Spinner {
    pub fn start(message: &str) -> Spinner {
        let message = Arc::new(Mutex::new(message.to_string()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::io::stderr().is_terminal().then(|| {
            let (message, stop) = (message.clone(), stop.clone());
            std::thread::spawn(move || {
                let started = Instant::now();
                let mut frame = 0;
                while !stop.load(Ordering::Relaxed) {
                    // Nothing for a quick job: only after a moment.
                    if started.elapsed() > Duration::from_millis(150) {
                        let text = message.lock().map(|m| m.clone()).unwrap_or_default();
                        let elapsed = started.elapsed().as_secs();
                        let time = if elapsed >= 2 { format!("  \x1b[2m{elapsed}s\x1b[0m") } else { String::new() };
                        let mut err = std::io::stderr().lock();
                        let _ = write!(err, "\r\x1b[2K\x1b[36m{}\x1b[0m {text}{time}", FRAMES[frame % FRAMES.len()]);
                        let _ = err.flush();
                        frame += 1;
                    }
                    std::thread::sleep(Duration::from_millis(80));
                }
                let mut err = std::io::stderr().lock();
                let _ = write!(err, "\r\x1b[2K");
                let _ = err.flush();
            })
        });
        Spinner { message, stop, thread }
    }

    pub fn set(&self, text: &str) {
        if let Ok(mut message) = self.message.lock() {
            *message = text.to_string();
        }
    }

    pub fn finish(&mut self) {
        self.end();
    }

    fn end(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.end();
    }
}
