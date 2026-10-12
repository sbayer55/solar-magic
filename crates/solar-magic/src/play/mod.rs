//! Playing the edited level: the emulator runs on its own thread, paced to
//! the console's frame rate, and hands finished pictures to the UI.

mod audio;
mod debug;
mod gamepad;
mod input;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, channel, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use smw_data::RomLayout;
use smw_play::{LaunchRequest, Launcher, Phase};
use snes_core::{Console, Hit, Input, Region, RunReport};
use snes_rom::Rom;

pub(crate) use debug::{DebugConfig, DebugSnapshot, TraceRing, parse_address};
pub(crate) use gamepad::Gamepads;
pub(crate) use input::{KeyMap, read_input};

/// Master cycles per NTSC frame (262 lines of 1364, minus the short line).
const FRAME_NANOS: u64 = 16_639_267;
/// Frames between rewind snapshots.
const REWIND_INTERVAL: u64 = 15;
/// Rewind snapshots kept (10 seconds).
const REWIND_DEPTH: usize = 40;
/// Frames between battery saves while the save RAM is dirty.
const BATTERY_SAVE_FRAMES: u64 = 300;

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
    /// Execute one instruction (while paused).
    StepInstruction,
    /// Stop when the program counter reaches this address.
    AddBreakpoint(u32),
    /// Forget a breakpoint.
    RemoveBreakpoint(u32),
    /// What the Debug panel wants captured.
    SetDebug(DebugConfig),
    /// Back to the previous rewind snapshot.
    Rewind,
    /// Exit the thread.
    Quit,
}

/// Text fields of the Debug panel.
#[derive(Debug, Clone, Default)]
pub(crate) struct DebugFields {
    pub(crate) breakpoint: String,
    pub(crate) watch: String,
    pub(crate) memory: String,
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
    /// Debugger view.
    pub(crate) debug: DebugSnapshot,
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
    /// What the Debug panel asks the thread to capture.
    pub(crate) debug: DebugConfig,
    /// The Debug panel's text fields.
    pub(crate) debug_fields: DebugFields,
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
        srm: Option<PathBuf>,
    ) -> Self {
        let (commands, command_rx) = channel();
        let (frame_tx, frames) = sync_channel(2);
        let worker = Worker { rom: rom.clone(), layout: layout.clone(), ctx: ctx.clone(), srm };
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
            debug: DebugConfig::default(),
            debug_fields: DebugFields::default(),
        }
    }

    /// Sends the Debug panel's current wishes to the thread.
    pub(crate) fn send_debug_config(&self) {
        self.send(Command::SetDebug(self.debug.clone()));
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
    /// Battery save file next to the ROM.
    srm: Option<PathBuf>,
}

/// Everything the thread mutates per frame.
struct Machine {
    console: Console,
    launcher: Launcher,
    input: Input,
    running: bool,
    steps_left: u32,
    saved: Option<(Console, Launcher)>,
    /// A snapshot every `REWIND_INTERVAL` frames, oldest first.
    history: VecDeque<(Console, Launcher)>,
    trace: TraceRing,
    debug: DebugConfig,
    /// Why the last run stopped.
    stop: Option<Hit>,
    /// Emit a frame although nothing ran (after a step or a rewind).
    refresh: bool,
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
        self.load_battery(&mut console);
        let launcher = Launcher::start(&mut console, &self.rom, &self.layout, request);
        let mut m = Machine {
            console,
            launcher,
            input: Input::default(),
            running: true,
            steps_left: 0,
            saved: None,
            history: VecDeque::new(),
            trace: TraceRing::default(),
            debug: DebugConfig::default(),
            stop: None,
            refresh: false,
        };
        let mut audio = audio::AudioOut::open();
        let mut samples = Vec::new();
        let mut pacer = Pacer::new();
        loop {
            let active = m.running || m.steps_left > 0;
            let command = if active {
                commands.try_recv().ok()
            } else {
                commands.recv_timeout(Duration::from_millis(50)).ok()
            };
            if let Some(command) = command
                && !self.handle(&mut m, command)
            {
                return;
            }
            let ran = m.running || m.steps_left > 0;
            if ran {
                m.advance(&self.layout);
                m.console.take_audio(&mut samples);
                match audio.as_mut() {
                    Some(out) => {
                        out.push(&samples);
                        pacer.wait_for_audio(out);
                    }
                    None => pacer.wait(),
                }
                samples.clear();
                if m.console.bus().clock.frame.is_multiple_of(BATTERY_SAVE_FRAMES) {
                    self.save_battery(&mut m.console);
                }
            } else if !m.refresh {
                continue;
            }
            m.refresh = false;
            match frames.try_send(m.event(&self.layout, pacer.fps)) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => return,
            }
            self.ctx.request_repaint();
        }
    }

    /// Applies a command; returns whether the thread keeps going.
    fn handle(&self, m: &mut Machine, command: Command) -> bool {
        match command {
            Command::Run => {
                m.running = true;
                m.stop = None;
            }
            Command::Pause => m.running = false,
            Command::Step(n) => {
                m.running = false;
                m.steps_left = n;
                m.stop = None;
            }
            Command::Relaunch(next) => {
                self.save_battery(&mut m.console);
                m.launcher = Launcher::start(&mut m.console, &self.rom, &self.layout, *next);
                m.history.clear();
                m.running = true;
                m.stop = None;
            }
            Command::Input(new_input) => m.input = new_input,
            Command::SaveState => m.saved = Some((m.console.clone(), m.launcher.clone())),
            Command::LoadState => {
                if let Some((c, l)) = m.saved.as_ref() {
                    m.console = c.clone();
                    m.launcher = l.clone();
                }
                m.refresh = true;
            }
            Command::StepInstruction => m.step_instruction(),
            Command::AddBreakpoint(addr) => {
                m.console.add_breakpoint(addr);
                m.refresh = true;
            }
            Command::RemoveBreakpoint(addr) => {
                m.console.remove_breakpoint(addr);
                m.refresh = true;
            }
            Command::SetDebug(cfg) => {
                m.debug = cfg;
                m.refresh = true;
            }
            Command::Rewind => m.rewind(),
            Command::Quit => {
                self.save_battery(&mut m.console);
                return false;
            }
        }
        true
    }

    /// Loads `<rom>.srm` into the cartridge's battery RAM when it exists.
    fn load_battery(&self, console: &mut Console) {
        if let Some(path) = self.srm.as_ref()
            && let Ok(bytes) = std::fs::read(path)
        {
            console.load_battery(&bytes);
        }
    }

    /// Writes the battery RAM to `<rom>.srm` when it changed.
    fn save_battery(&self, console: &mut Console) {
        if let Some(path) = self.srm.as_ref()
            && console.battery_dirty()
            && !console.battery().is_empty()
            && std::fs::write(path, console.battery()).is_ok()
        {
            console.clear_battery_dirty();
        }
    }
}

impl Machine {
    /// Runs one frame (or one launch step), stopping on a breakpoint.
    fn advance(&mut self, layout: &RomLayout) {
        self.steps_left = self.steps_left.saturating_sub(1);
        self.console.set_input(self.input);
        if self.launcher.phase() != Phase::Playing {
            self.launcher.step(&mut self.console, layout);
            return;
        }
        self.record_history();
        let report = if self.debug.trace {
            self.run_frame_traced()
        } else {
            self.console.run_frame(self.input)
        };
        if let Some(hit) = report.hit {
            self.running = false;
            self.steps_left = 0;
            self.stop = Some(hit);
        }
    }

    fn run_frame_traced(&mut self) -> RunReport {
        let target = self.console.bus().clock.frame.wrapping_add(1);
        let trace = &mut self.trace;
        self.console.run_while(self.input, |c| {
            trace.push(c);
            c.bus().clock.frame < target
        })
    }

    fn record_history(&mut self) {
        if !self.console.bus().clock.frame.is_multiple_of(REWIND_INTERVAL) {
            return;
        }
        if self.history.len() >= REWIND_DEPTH {
            self.history.pop_front();
        }
        self.history.push_back((self.console.clone(), self.launcher.clone()));
    }

    fn step_instruction(&mut self) {
        if self.launcher.phase() == Phase::Playing {
            if self.debug.trace {
                self.trace.push(&self.console);
            }
            self.stop = self.console.step_instruction().err().map(Hit::Stopped);
        }
        self.refresh = true;
    }

    fn rewind(&mut self) {
        if let Some((c, l)) = self.history.pop_back() {
            self.console = c;
            self.launcher = l;
        }
        self.stop = None;
        self.refresh = true;
    }

    fn event(&self, layout: &RomLayout, fps: f32) -> FrameEvent {
        let frame = self.console.frame();
        FrameEvent {
            rgba: Arc::from(frame.to_rgba()),
            width: frame.width(),
            height: frame.height(),
            frame: self.console.bus().clock.frame,
            phase: self.launcher.phase(),
            game_mode: smw_play::game_mode(&self.console, layout),
            player: smw_play::player_position(&self.console, layout),
            fps,
            debug: DebugSnapshot::capture(&self.console, &self.debug, &self.trace, self.stop),
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
        let request = LaunchRequest { level, mario: smw_play::MarioState::default(), pixel: None };
        let mut session = PlaySession::start(&ctx, &rom, &layout, request, None);
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
