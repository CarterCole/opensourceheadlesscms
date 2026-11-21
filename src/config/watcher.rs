use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct ConfigChanged;

pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    pub rx: broadcast::Receiver<ConfigChanged>,
}

impl ConfigWatcher {
    pub fn start(dirs: &[&str]) -> Result<(Self, broadcast::Sender<ConfigChanged>), notify::Error> {
        let (tx, rx) = broadcast::channel::<ConfigChanged>(16);
        let tx_clone = tx.clone();

        let (fs_tx, fs_rx) = mpsc::channel::<Result<Event, notify::Error>>();
        let mut watcher = notify::recommended_watcher(fs_tx)?;

        for dir in dirs {
            if std::path::Path::new(dir).exists() {
                watcher.watch(std::path::Path::new(dir), RecursiveMode::Recursive)?;
                info!("watching config dir: {dir}");
            } else {
                warn!("skipping watch for missing dir: {dir}");
            }
        }

        std::thread::spawn(move || {
            // debounce: collect events for 200ms before firing
            loop {
                if let Ok(Ok(_event)) = fs_rx.recv_timeout(Duration::from_millis(200)) {
                    // drain any batched events
                    while fs_rx.try_recv().is_ok() {}
                    let _ = tx_clone.send(ConfigChanged);
                }
            }
        });

        Ok((
            Self {
                _watcher: watcher,
                rx,
            },
            tx,
        ))
    }
}
