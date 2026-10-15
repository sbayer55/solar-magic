//! The Debug panel: S-CPU registers, disassembly with click-to-break,
//! breakpoints, watches, a memory page, the instruction trace, and
//! single-step / rewind controls.

use egui::RichText;

use super::Viewer;
use crate::play::{Command, DebugSnapshot, PlaySession, parse_address};

impl Viewer<'_> {
    pub(super) fn debug_panel(&mut self, ui: &mut egui::Ui) {
        let doc = &mut *self.doc;
        let Some(play) = doc.play.as_mut() else {
            ui.label("Start a level (Tools → Play Level) to debug it.");
            return;
        };
        play.poll(ui.ctx());
        if !play.debug.wanted {
            play.debug.wanted = true;
            play.send_debug_config();
        }
        controls(ui, play);
        let Some(snapshot) = play.latest.as_ref().map(|f| f.debug.clone()) else {
            ui.spinner();
            return;
        };
        if let Some(stop) = snapshot.stop.as_ref() {
            ui.colored_label(egui::Color32::LIGHT_RED, stop);
        }
        registers(ui, &snapshot);
        egui::ScrollArea::vertical().show(ui, |ui| {
            disassembly(ui, play, &snapshot);
            breakpoints(ui, play, &snapshot);
            watches(ui, play, &snapshot);
            memory(ui, play, &snapshot);
            trace(ui, play, &snapshot);
        });
    }
}

fn controls(ui: &mut egui::Ui, play: &mut PlaySession) {
    ui.horizontal(|ui| {
        let label = if play.running { "Pause" } else { "Run" };
        if ui.button(label).clicked() {
            play.running = !play.running;
            play.send(if play.running { Command::Run } else { Command::Pause });
        }
        if ui.add_enabled(!play.running, egui::Button::new("Step instruction")).clicked() {
            play.send(Command::StepInstruction);
        }
        if ui.add_enabled(!play.running, egui::Button::new("Step frame")).clicked() {
            play.send(Command::Step(1));
        }
        if ui.button("Rewind").on_hover_text("Back a quarter second").clicked() {
            play.send(Command::Rewind);
        }
        if ui.checkbox(&mut play.debug.trace, "Trace").changed() {
            play.send_debug_config();
        }
    });
}

fn registers(ui: &mut egui::Ui, snapshot: &DebugSnapshot) {
    let cpu = snapshot.cpu;
    let text = format!(
        "A={:04X} X={:04X} Y={:04X} S={:04X} D={:04X} DB={:02X} PB={:02X} PC={:04X}  P={} E={}{}  #{}",
        cpu.a,
        cpu.x,
        cpu.y,
        cpu.s,
        cpu.d,
        cpu.db,
        cpu.pb,
        cpu.pc,
        cpu.flags(),
        u8::from(cpu.e),
        if cpu.waiting { " WAI" } else { "" },
        cpu.steps
    );
    ui.label(RichText::new(text).monospace());
}

fn disassembly(ui: &mut egui::Ui, play: &PlaySession, snapshot: &DebugSnapshot) {
    ui.separator();
    ui.small("Click a line to toggle a breakpoint.");
    for (i, line) in snapshot.disasm.iter().enumerate() {
        let is_bp = snapshot.breakpoints.contains(&(line.pc24 & 0x7F_FFFF));
        let marker = if is_bp { "●" } else { " " };
        let text = format!("{marker} {:06X}  {}", line.pc24, line.text);
        let mut rich = RichText::new(text).monospace();
        if i == 0 {
            rich = rich.strong();
        }
        if ui.add(egui::Label::new(rich).sense(egui::Sense::click())).clicked() {
            play.send(if is_bp {
                Command::RemoveBreakpoint(line.pc24)
            } else {
                Command::AddBreakpoint(line.pc24)
            });
        }
    }
}

fn breakpoints(ui: &mut egui::Ui, play: &mut PlaySession, snapshot: &DebugSnapshot) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Breakpoint");
        ui.add(egui::TextEdit::singleline(&mut play.debug_fields.breakpoint).desired_width(80.0));
        if ui.button("Add").clicked()
            && let Some(addr) = parse_address(&play.debug_fields.breakpoint)
        {
            play.send(Command::AddBreakpoint(addr));
            play.debug_fields.breakpoint.clear();
        }
    });
    for &bp in &snapshot.breakpoints {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("${bp:06X}")).monospace());
            if ui.small_button("×").clicked() {
                play.send(Command::RemoveBreakpoint(bp));
            }
        });
    }
}

fn watches(ui: &mut egui::Ui, play: &mut PlaySession, snapshot: &DebugSnapshot) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Watch");
        ui.add(egui::TextEdit::singleline(&mut play.debug_fields.watch).desired_width(80.0));
        if ui.button("Add").clicked()
            && let Some(addr) = parse_address(&play.debug_fields.watch)
        {
            play.debug.watches.push(addr);
            play.debug_fields.watch.clear();
            play.send_debug_config();
        }
    });
    let mut remove = None;
    for (i, &(addr, value)) in snapshot.watches.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("${addr:06X} = {value:02X}")).monospace());
            if ui.small_button("×").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove
        && i < play.debug.watches.len()
    {
        play.debug.watches.remove(i);
        play.send_debug_config();
    }
}

fn memory(ui: &mut egui::Ui, play: &mut PlaySession, snapshot: &DebugSnapshot) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Memory");
        let edit =
            ui.add(egui::TextEdit::singleline(&mut play.debug_fields.memory).desired_width(80.0));
        if (edit.lost_focus() || ui.button("Go").clicked())
            && let Some(addr) = parse_address(&play.debug_fields.memory)
        {
            play.debug.memory_base = addr & 0xFF_FFF0;
            play.send_debug_config();
        }
    });
    for (row, chunk) in snapshot.memory.chunks(16).enumerate() {
        let base =
            snapshot.memory_base.wrapping_add(u32::try_from(row).unwrap_or(0).wrapping_mul(16));
        let bytes: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| if (0x20..0x7F).contains(&b) { char::from(b) } else { '.' })
            .collect();
        ui.label(RichText::new(format!("{base:06X}  {}  {ascii}", bytes.join(" "))).monospace());
    }
}

fn trace(ui: &mut egui::Ui, play: &PlaySession, snapshot: &DebugSnapshot) {
    if !play.debug.trace {
        return;
    }
    ui.separator();
    ui.label("Trace (oldest first)");
    for line in &snapshot.trace {
        ui.label(RichText::new(format!("{:06X}  {}", line.pc24, line.text)).monospace());
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use smw_data::LevelId;
    use smw_play::Phase;

    use crate::document::Document;
    use crate::panels::{ViewState, Viewer};

    /// With `SMW_ROM`, the Debug panel draws headlessly over a running
    /// level: registers, ten disassembled lines and a memory page arrive.
    #[test]
    #[allow(clippy::expect_used, clippy::print_stderr, reason = "test harness")]
    fn debug_panel_draws_a_running_level() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping debug panel test");
            return;
        };
        let ctx = egui::Context::default();
        let mut doc = Document::open(std::path::Path::new(&path), LevelId::new(0x105).expect("id"))
            .expect("document");
        doc.start_play(&ctx, smw_play::Entrance::Main, None);
        let deadline = Instant::now().checked_add(Duration::from_secs(45)).expect("deadline");
        loop {
            assert!(Instant::now() < deadline, "the level did not start");
            let play = doc.play.as_mut().expect("session");
            play.poll(&ctx);
            if play.latest.as_ref().is_some_and(|f| f.phase == Phase::Playing) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let play = doc.play.as_mut().expect("session");
        play.send(crate::play::Command::Pause);
        play.debug.watches.push(0x7E_0019);
        play.debug.memory_base = 0x7E_0000;
        play.send_debug_config();
        std::thread::sleep(Duration::from_millis(200));
        let mut view = ViewState::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut viewer = Viewer { doc: &mut doc, view: &mut view, actions: Vec::new() };
                viewer.debug_panel(ui);
            });
        });
        output.textures_delta.clear();
        // Drawing the panel asked the thread for snapshots; the next frame carries one.
        let snapshot_due = Instant::now().checked_add(Duration::from_secs(5)).expect("deadline");
        loop {
            let session = doc.play.as_mut().expect("session");
            session.poll(&ctx);
            if session.latest.as_ref().is_some_and(|f| !f.debug.watches.is_empty()) {
                break;
            }
            assert!(Instant::now() < snapshot_due, "no snapshot with the watch arrived");
            std::thread::sleep(Duration::from_millis(20));
        }
        let debug = &doc.play.as_ref().expect("session").latest.as_ref().expect("frame").debug;
        assert_eq!(debug.disasm.len(), 10);
        assert_eq!(debug.memory.len(), 256);
        assert_eq!(debug.watches.first().map(|w| w.0), Some(0x7E_0019));
        assert!(debug.disasm.first().is_some_and(|l| l.pc24 & 0xFF_FFFF == l.pc24));
    }
}
