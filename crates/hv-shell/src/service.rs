//! Background inspection of directories for the prompt chips: file reads never run on the UI
//! thread (CLAUDE.md), and results are cached briefly per directory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use crate::detect::{ProjectInfo, inspect};

/// How long a directory's result is reused (a prompt per command must not re-read the disk).
const CACHE_FOR: Duration = Duration::from_secs(3);
/// Most directories kept in the cache.
const CACHE_SIZE: usize = 256;

/// One request: an opaque tag returned with the result (the app uses pane and line).
#[derive(Debug)]
struct Request<T> {
    tag: T,
    dir: PathBuf,
}

/// Runs [`inspect`] on a worker thread.
#[derive(Debug)]
pub struct ChipService<T: Send + 'static> {
    requests: Sender<Request<T>>,
}

impl<T: Send + 'static> ChipService<T> {
    /// Starts the worker. `deliver` runs on the worker thread with each result.
    pub fn start(
        home: Option<PathBuf>,
        deliver: impl Fn(T, Arc<ProjectInfo>) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (tx, rx) = channel::<Request<T>>();
        std::thread::Builder::new()
            .name("hv-chips".into())
            .spawn(move || {
                let mut cache: HashMap<PathBuf, (Instant, Arc<ProjectInfo>)> = HashMap::new();
                for request in rx {
                    let now = Instant::now();
                    let info = match cache.get(&request.dir) {
                        Some((at, info)) if now.duration_since(*at) < CACHE_FOR => Arc::clone(info),
                        _ => {
                            let info = Arc::new(inspect(&request.dir, home.as_deref()));
                            if cache.len() >= CACHE_SIZE {
                                cache.retain(|_, (at, _)| now.duration_since(*at) < CACHE_FOR);
                            }
                            cache.insert(request.dir.clone(), (now, Arc::clone(&info)));
                            info
                        }
                    };
                    deliver(request.tag, info);
                }
            })?;
        Ok(Self { requests: tx })
    }

    /// Asks for the chips of `dir`; the answer arrives through `deliver` with `tag`.
    pub fn request(&self, tag: T, dir: PathBuf) {
        let _ = self.requests.send(Request { tag, dir });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn results_arrive_with_their_tag() {
        let (tx, rx) = channel();
        let tx = Mutex::new(tx);
        let service = ChipService::start(None, move |tag: u32, info| {
            let _ = tx.lock().unwrap().send((tag, info));
        })
        .unwrap();
        let dir = std::env::temp_dir();
        service.request(7, dir.clone());
        service.request(8, dir);
        let (a, first) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (b, second) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((a, b), (7, 8));
        // The second request hits the cache: same allocation.
        assert!(Arc::ptr_eq(&first, &second));
    }
}
