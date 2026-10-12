//! Playing the edited level: the emulator runs on its own thread, paced to
//! the console's frame rate, and hands finished pictures to the UI.

mod audio;
mod gamepad;
mod input;

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, channel, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use smw_data::RomLayout;
use smw_play::{LaunchRequest, Launcher, Phase};
use snes_core::{Console, Input, Region};
use snes_rom::Rom;

pub(crate) use gamepad::Gamepads;
pub(crate) use input::{KeyMap, read_input};

/// Master cycles per NTSC frame (262 lines of 1364, minus the short line).
const FRAME_NANOS: u64 = 16_639_267;

/// What the UI asks the emulator thread to do.
#[derive(Debug)]
pub(crate) enum Command {
    /// Run continuously.
    Run,
    /// Stop after the current frame.
    Pause,
    /// Run this many frames, then pause.
    Step(u32),
    /// Start over with a new request.
    Relaunch(Box<LaunchRequest>),
    /// Controller state.
    Input(Input),
    /// Remember the machine state.
    SaveState,
    /// Return to the remembered state.
    LoadState,
    /// Exit the thread.
    Quit,
}

/// A finished frame and where the machine is.
#[derive(Debug, Clone)]
pub(crate) struct FrameEvent {
    /// Tightly packed RGBA, `width × height`.
    pub(crate) rgba: Arc<[u8]>,
    /// Columns (256, or 512 in hi-res).
    pub(crate) width: usize,
    /// Rows (224/239, doubled when interlaced).
    pub(crate) height: usize,
    /// Frame counter.
    pub(crate) frame: u64,
    /// Launch phase.
    pub(crate) phase: Phase,
    /// Game mode byte.
    pub(crate) game_mode: u8,
    /// Player position.
    pub(crate) player: (u16, u16),
    /// Emulated frames per wall-clock second over the last second.
    pub(crate) fps: f32,
}

/// The UI's handle on a running emulator.
pub(crate) struct PlaySession {
    commands: Sender<Command>,
    frames: Receiver<FrameEvent>,
    thread: Option<JoinHandle<()>>,
    /// Last frame received.
    pub(crate) latest: Option<FrameEvent>,
    /// Texture holding `latest`.
    pub(crate) texture: Option<egui::TextureHandle>,
    /// Whether the thread is running frames.
    pub(crate) running: bool,
    /// Integer zoom.
    pub(crate) zoom: u8,
    /// Last controller state sent.
    pub(crate) input: Input,
    /// Key bindings.
    pub(crate) keys: KeyMap,
    /// Gamepads (feature `gamepad`).
    pub(crate) gamepads: Option<Gamepads>,
}

impl std::fmt::Debug for PlaySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaySession").field("running", &self.running).finish_non_exhaustive()
    }
}

impl PlaySession {
    /// Spawns the emulator thread and launches `request`.
    pub(crate) fn start(
        ctx: &egui::Context,
        rom: &Rom,
        layout: &RomLayout,
        request: LaunchRequest,
    ) -> Self {
        let (commands, command_rx) = channel();
        let (frame_tx, frames) = sync_channel(2);
        let worker = Worker { rom: rom.clone(), layout: layout.clone(), ctx: ctx.clone() };
        let thread = std::thread::Builder::new()
            .name("snes".to_owned())
            .spawn(move || worker.run(request, &command_rx, &frame_tx))
            .ok();
        Self {
            commands,
            frames,
            thread,
            latest: None,
            texture: None,
            running: true,
            zoom: 2,
            input: Input::default(),
            keys: KeyMap::default(),
            gamepads: Gamepads::new(),
        }
    }

    /// Sends a command (ignoring a dead thread).
    pub(crate) fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Pulls the newest frame and refreshes the texture.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        let mut newest = None;
        while let Ok(frame) = self.frames.try_recv() {
            newest = Some(frame);
        }
        if let Some(frame) = newest {
            let size = [frame.width, frame.height];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &frame.rgba);
            match self.texture.as_mut() {
                Some(texture) if texture.size() == size => {
                    texture.set(image, egui::TextureOptions::NEAREST);
                }
                _ => {
                    self.texture =
                        Some(ctx.load_texture("play", image, egui::TextureOptions::NEAREST));
                }
            }
            self.latest = Some(frame);
        }
    }

    /// Sends the controller state when it changed.
    pub(crate) fn set_input(&mut self, input: Input) {
        if input != self.input {
            self.input = input;
            self.send(Command::Input(input));
        }
    }
}

impl Drop for PlaySession {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// State owned by the emulator thread.
struct Worker {
    rom: Rom,
    layout: RomLayout,
    ctx: egui::Context,
}

/// Frame-rate accounting.
struct Pacer {
    next: Instant,
    window_start: Instant,
    window_frames: u32,
    fps: f32,
}

impl Pacer {
    fn new() -> Self {
        let now = Instant::now();
        Self { next: now, window_start: now, window_frames: 0, fps: 0.0 }
    }

    /// Paces on the audio queue: sleeps while the device has more than the
    /// target depth queued, so the emulator runs exactly as fast as sound
    /// is consumed.
    fn wait_for_audio(&mut self, out: &audio::AudioOut) {
        while out.queued_frames() > audio::TARGET_FRAMES {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.count_frame(Instant::now());
    }

    /// Sleeps until the next frame is due; resynchronises when far behind.
    fn wait(&mut self) {
        let now = Instant::now();
        if self.next > now {
            std::thread::sleep(self.next.saturating_duration_since(now));
        } else if now.saturating_duration_since(self.next) > Duration::from_millis(100) {
            self.next = now;
        }
        self.next = self.next.checked_add(Duration::from_nanos(FRAME_NANOS)).unwrap_or(now);
        self.count_frame(now);
    }

    fn count_frame(&mut self, now: Instant) {
        self.window_frames = self.window_frames.saturating_add(1);
        let elapsed = now.saturating_duration_since(self.window_start);
        if elapsed >= Duration::from_secs(1) {
            self.fps = f32::from(u16::try_from(self.window_frames).unwrap_or(u16::MAX))
                / elapsed.as_secs_f32();
            self.window_start = now;
            self.window_frames = 0;
        }
    }
}

impl Worker {
    fn run(
        self,
        request: LaunchRequest,
        commands: &Receiver<Command>,
        frames: &SyncSender<FrameEvent>,
    ) {
        let mut console = Console::new(&self.rom, Region::Ntsc);
        let mut audio = audio::AudioOut::open();
        let mut samples = Vec::new();
        let mut launcher = Launcher::start(&mut console, &self.rom, &self.layout, request);
        let mut input = Input::default();
        let mut running = true;
        let mut steps_left = 0u32;
        let mut saved: Option<(Console, Launcher)> = None;
        let mut pacer = Pacer::new();
        loop {
            let command = if running || steps_left > 0 {
                commands.try_recv().ok()
            } else {
                commands.recv_timeout(Duration::from_millis(50)).ok()
            };
            match command {
                Some(Command::Run) => running = true,
                Some(Command::Pause) => running = false,
                Some(Command::Step(n)) => {
                    running = false;
                    steps_left = n;
                }
                Some(Command::Relaunch(next)) => {
                    launcher = Launcher::start(&mut console, &self.rom, &self.layout, *next);
                    running = true;
                }
                Some(Command::Input(new_input)) => input = new_input,
                Some(Command::SaveState) => saved = Some((console.clone(), launcher.clone())),
                Some(Command::LoadState) => {
                    if let Some((c, l)) = saved.as_ref() {
                        console = c.clone();
                        launcher = l.clone();
                    }
                }
                Some(Command::Quit) => return,
                None => {}
            }
            if !running && steps_left == 0 {
                continue;
            }
            steps_left = steps_left.saturating_sub(1);
            console.set_input(input);
            if launcher.phase() == Phase::Playing {
                console.run_frame(input);
            } else {
                launcher.step(&mut console, &self.layout);
            }
            console.take_audio(&mut samples);
            match audio.as_mut() {
                Some(out) => {
                    out.push(&samples);
                    pacer.wait_for_audio(out);
                }
                None => pacer.wait(),
            }
            samples.clear();
            let frame = console.frame();
            let event = FrameEvent {
                rgba: Arc::from(frame.to_rgba()),
                width: frame.width(),
                height: frame.height(),
                frame: console.bus().clock.frame,
                phase: launcher.phase(),
                game_mode: smw_play::game_mode(&console, &self.layout),
                player: smw_play::player_position(&console, &self.layout),
                fps: pacer.fps,
            };
            match frames.try_send(event) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => return,
            }
            self.ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With `SMW_ROM`, a session boots, launches level 105 and delivers
    /// playing frames through a headless egui context.
    #[test]
    #[allow(clippy::expect_used, clippy::print_stderr, reason = "test harness")]
    fn session_reaches_play() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping play session test");
            return;
        };
        let rom = Rom::load(std::path::Path::new(&path)).expect("ROM loads");
        let layout = RomLayout::detect(&rom).expect("layout");
        let level =
            smw_data::level::Level::load(&rom, &layout, smw_data::LevelId::new(0x105).expect("id"))
                .expect("level");
        let ctx = egui::Context::default();
        let request = LaunchRequest { level, mario: smw_play::MarioState::default() };
        let mut session = PlaySession::start(&ctx, &rom, &layout, request);
        let deadline = Instant::now().checked_add(Duration::from_secs(45)).expect("deadline");
        let mut playing = None;
        while Instant::now() < deadline {
            session.poll(&ctx);
            if let Some(frame) = session.latest.as_ref()
                && frame.phase == Phase::Playing
            {
                playing = Some(frame.clone());
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let frame = playing.expect("the level started within a minute");
        assert_eq!(frame.game_mode, 0x14);
        assert_eq!((frame.width, frame.height), (256, 224));
        assert!(frame.rgba.chunks(4).any(|p| p != [0, 0, 0, 0xFF]));
        session.send(Command::Pause);
    }
}
