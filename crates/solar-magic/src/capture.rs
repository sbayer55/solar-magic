//! Capturing the machine's picture of the edited level on a worker thread,
//! so the editor stays responsive while the emulator boots and fades in.

use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use smw_data::RomLayout;
use smw_data::level::Level;
use smw_play::{BootCache, CaptureOptions, capture_level};
use smw_render::assets::LevelAssets;
use smw_render::scene::SceneKey;
use snes_rom::Rom;

/// What one capture needs.
pub(crate) struct CaptureJob {
    /// The ROM to boot.
    pub(crate) rom: Rom,
    /// Its layout.
    pub(crate) layout: RomLayout,
    /// The level as edited (overlaid on the ROM by the launcher).
    pub(crate) level: Level,
    /// What the result belongs to.
    pub(crate) key: SceneKey,
    /// The booted console shared with other captures.
    pub(crate) boot: Arc<Mutex<BootCache>>,
}

/// The captured assets, or why there are none.
pub(crate) type CaptureResult = Result<Arc<LevelAssets>, String>;

/// A capture running on its own thread.
pub(crate) struct CaptureSession {
    key: SceneKey,
    started: Instant,
    result: Receiver<CaptureResult>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for CaptureSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureSession").field("key", &self.key).finish_non_exhaustive()
    }
}

impl CaptureSession {
    /// Spawns the capture; the thread asks for a repaint when it finishes.
    pub(crate) fn start(ctx: &egui::Context, job: CaptureJob) -> Self {
        let (tx, result) = sync_channel(1);
        let key = job.key.clone();
        let ctx = ctx.clone();
        let thread = std::thread::Builder::new()
            .name("capture".to_owned())
            .spawn(move || {
                let _ = tx.send(run(job));
                ctx.request_repaint();
            })
            .ok();
        Self { key, started: Instant::now(), result, thread }
    }

    /// What the capture is for.
    pub(crate) const fn key(&self) -> &SceneKey {
        &self.key
    }

    /// How long it has been running.
    pub(crate) fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// The result once the thread has finished, else `None`.
    pub(crate) fn try_take(&mut self) -> Option<CaptureResult> {
        let outcome = self.result.try_recv().ok()?;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        Some(outcome)
    }
}

fn run(job: CaptureJob) -> CaptureResult {
    let mut boot = job.boot.lock().map_err(|_| "boot cache poisoned".to_owned())?;
    capture_level(&mut boot, &job.rom, &job.layout, job.level, CaptureOptions::default())
        .map(Arc::new)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use smw_data::level::Level;
    use smw_data::{LevelId, RomLayout};
    use smw_play::BootCache;
    use smw_render::scene::SceneKey;

    use super::{CaptureJob, CaptureSession};

    /// Assets for level 105 arrive on the worker within the timeout.
    #[test]
    #[allow(clippy::expect_used, clippy::print_stderr, reason = "test harness")]
    fn capture_session_delivers_assets() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = RomLayout::detect(&rom).expect("layout");
        let level = Level::load(&rom, &layout, LevelId::new(0x105).expect("valid")).expect("level");
        let key = SceneKey::of(&level);
        let job = CaptureJob {
            rom,
            layout,
            level,
            key: key.clone(),
            boot: Arc::new(Mutex::new(BootCache::new())),
        };
        let mut session = CaptureSession::start(&egui::Context::default(), job);
        assert_eq!(session.key(), &key);
        let deadline = Instant::now() + Duration::from_mins(10);
        let outcome = loop {
            if let Some(outcome) = session.try_take() {
                break outcome;
            }
            assert!(Instant::now() < deadline, "capture timed out");
            std::thread::sleep(Duration::from_millis(100));
        };
        let assets = outcome.expect("captures");
        assert_eq!(assets.play.bg_mode, 1);
    }
}
