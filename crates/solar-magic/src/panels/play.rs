//! The Play panel: the emulated picture, transport controls and status.

use egui::Sense;
use smw_play::Phase;

use super::Viewer;
use crate::play::{Command, PlaySession, read_input};

impl Viewer<'_> {
    pub(super) fn play_panel(&mut self, ui: &mut egui::Ui) {
        let doc = &mut *self.doc;
        let Some(play) = doc.play.as_mut() else {
            ui.label("Tools → Play Level starts the current level in the emulator.");
            return;
        };
        play.poll(ui.ctx());
        transport(ui, play);
        let Some(texture) = play.texture.as_ref() else {
            ui.spinner();
            return;
        };
        let zoom = f32::from(play.zoom.max(1));
        let size = egui::vec2(
            super::to_f32(u32::try_from(texture.size()[0]).unwrap_or(256)) * zoom,
            super::to_f32(u32::try_from(texture.size()[1]).unwrap_or(224)) * zoom,
        );
        let response =
            ui.add(egui::Image::new(texture).fit_to_exact_size(size).sense(Sense::click()));
        if response.clicked() {
            response.request_focus();
        }
        if response.has_focus() || response.hovered() {
            let input = read_input(ui, &play.keys);
            play.set_input(input);
        } else {
            play.set_input(snes_core::Input::default());
        }
        status(ui, play);
    }
}

fn transport(ui: &mut egui::Ui, play: &mut PlaySession) {
    ui.horizontal(|ui| {
        let label = if play.running { "Pause" } else { "Run" };
        if ui.button(label).clicked() {
            play.running = !play.running;
            play.send(if play.running { Command::Run } else { Command::Pause });
        }
        if ui.add_enabled(!play.running, egui::Button::new("Frame")).clicked() {
            play.send(Command::Step(1));
        }
        if ui.button("Save state").on_hover_text("Remember the machine (in memory)").clicked() {
            play.send(Command::SaveState);
        }
        if ui.button("Load state").clicked() {
            play.send(Command::LoadState);
        }
        ui.label("Zoom");
        for z in 1..=3u8 {
            if ui.selectable_label(play.zoom == z, format!("{z}×")).clicked() {
                play.zoom = z;
            }
        }
    });
}

fn status(ui: &mut egui::Ui, play: &PlaySession) {
    let Some(frame) = play.latest.as_ref() else { return };
    let phase = match frame.phase {
        Phase::Boot => "booting",
        Phase::WaitEntry => "entering level",
        Phase::WaitLevel => "loading level",
        Phase::Playing => "playing",
        Phase::Failed(_) => "launch failed",
    };
    ui.horizontal(|ui| {
        ui.label(format!(
            "{phase}  ·  frame {}  ·  {:.0} fps  ·  mode {:02X}  ·  player ({}, {})",
            frame.frame, frame.fps, frame.game_mode, frame.player.0, frame.player.1
        ));
    });
    if let Phase::Failed(e) = frame.phase {
        ui.colored_label(egui::Color32::LIGHT_RED, e.to_string());
    }
    ui.small(crate::play::KeyMap::summary());
}
