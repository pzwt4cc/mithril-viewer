use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
};

use winit::event_loop::EventLoopProxy;

use crate::{
    UserEvent,
    loader::{self, Loaded},
};

const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;

struct Job {
    epoch: u64,
    paths: Vec<PathBuf>,
    shutdown: bool,
}

pub struct Prefetcher {
    cache: HashMap<PathBuf, Loaded>,
    epoch: Arc<AtomicU64>,
    state: Arc<(Mutex<Job>, Condvar)>,
    handle: Option<JoinHandle<()>>,
}

impl Prefetcher {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Self::with_sink(move |job_epoch, path, loaded| {
            let _ = proxy.send_event(UserEvent::Prefetched(job_epoch, path, loaded));
        })
    }

    pub fn with_sink<F>(sink: F) -> Self
    where
        F: Fn(u64, PathBuf, Box<Loaded>) + Send + 'static,
    {
        let epoch = Arc::new(AtomicU64::new(0));
        let state = Arc::new((
            Mutex::new(Job {
                epoch: 0,
                paths: Vec::new(),
                shutdown: false,
            }),
            Condvar::new(),
        ));

        let worker_epoch = Arc::clone(&epoch);
        let worker_state = Arc::clone(&state);
        let handle = thread::Builder::new()
            .name("mithril-prefetch".into())
            .spawn(move || {
                let mut last_epoch = 0u64;
                loop {
                    let (job_epoch, paths) = {
                        let (lock, cvar) = &*worker_state;
                        let mut guard = lock.lock().unwrap();
                        while !guard.shutdown && guard.epoch == last_epoch {
                            guard = cvar.wait(guard).unwrap();
                        }
                        if guard.shutdown {
                            return;
                        }
                        last_epoch = guard.epoch;
                        (guard.epoch, guard.paths.clone())
                    };

                    for path in paths {
                        if worker_epoch.load(Ordering::Relaxed) != job_epoch {
                            break;
                        }
                        if let Ok(loaded) = loader::load(&path) {
                            if worker_epoch.load(Ordering::Relaxed) != job_epoch {
                                break;
                            }
                            if loaded_bytes(&loaded) <= MAX_CACHE_BYTES {
                                sink(job_epoch, path, Box::new(loaded));
                            }
                        }
                    }
                }
            })
            .ok();

        Self {
            cache: HashMap::new(),
            epoch,
            state,
            handle,
        }
    }

    pub fn take(&mut self, path: &Path) -> Option<Loaded> {
        self.cache.remove(path)
    }

    pub fn invalidate(&mut self, path: &Path) {
        self.cache.remove(path);
    }

    pub fn clear(&mut self) {
        self.epoch.fetch_add(1, Ordering::Relaxed);
        self.cache.clear();
    }

    pub fn request(&mut self, targets: Vec<PathBuf>) {
        self.cache.retain(|k, _| targets.iter().any(|t| t == k));
        let missing: Vec<PathBuf> = targets
            .into_iter()
            .filter(|p| !self.cache.contains_key(p))
            .collect();
        let next_epoch = self.epoch.fetch_add(1, Ordering::Relaxed) + 1;
        if missing.is_empty() {
            return;
        }
        let (lock, cvar) = &*self.state;
        let mut guard = lock.lock().unwrap();
        guard.epoch = next_epoch;
        guard.paths = missing;
        cvar.notify_one();
    }

    pub fn insert(&mut self, job_epoch: u64, path: PathBuf, loaded: Loaded) {
        if job_epoch == self.epoch.load(Ordering::Relaxed) && self.cache.len() < 2 {
            self.cache.insert(path, loaded);
        }
    }

    pub fn shutdown(&mut self) {
        self.epoch.fetch_add(1, Ordering::Relaxed);
        {
            let (lock, cvar) = &*self.state;
            let mut guard = lock.lock().unwrap();
            guard.shutdown = true;
            cvar.notify_one();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Prefetcher {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn loaded_bytes(loaded: &Loaded) -> usize {
    let base = loaded.image.as_raw().len();
    let anim = loaded.anim.as_ref().map_or(0, |a| a.memory_bytes());
    base + anim
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbaImage};
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn test_prefetch_cache_eviction_and_size() {
        let mut cache: HashMap<PathBuf, Loaded> = HashMap::new();
        let make = || Loaded {
            image: RgbaImage::new(4, 4),
            format: Some(ImageFormat::Png),
            has_alpha: true,
            anim: None,
        };
        let p1 = PathBuf::from("a.png");
        let p2 = PathBuf::from("b.png");
        let p3 = PathBuf::from("c.png");
        cache.insert(p1.clone(), make());
        cache.insert(p2.clone(), make());

        let targets = [p2.clone(), p3];
        cache.retain(|k, _| targets.iter().any(|t| t == k));
        assert!(!cache.contains_key(&p1));
        assert!(cache.contains_key(&p2));
        assert!(loaded_bytes(&make()) < MAX_CACHE_BYTES);
    }

    #[test]
    fn test_prefetch_worker_and_epoch_filter() {
        let (tx, rx) = mpsc::channel();
        let mut pf = Prefetcher::with_sink(move |ep, path, loaded| {
            let _ = tx.send((ep, path, loaded));
        });
        let svg_path = PathBuf::from("assets/logo.svg");
        pf.request(vec![svg_path.clone()]);
        let (ep, path, loaded) = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(path, svg_path);

        pf.insert(ep.saturating_sub(1), PathBuf::from("stale.png"), *loaded);
        assert!(pf.take(Path::new("stale.png")).is_none());

        let fresh = Loaded {
            image: RgbaImage::new(2, 2),
            format: Some(ImageFormat::Png),
            has_alpha: false,
            anim: None,
        };
        pf.insert(ep, svg_path.clone(), fresh);
        assert!(pf.take(&svg_path).is_some());
        pf.shutdown();
    }
}
