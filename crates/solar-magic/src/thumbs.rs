//! Level thumbnails for the gallery: a worker thread runs the game's loader
//! on each requested level, renders it, and hands back a small picture of
//! its first screens.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use smw_data::{LevelId, RomLayout};
use smw_render::scene::{LevelScene, RenderOptions};
use smw_render::{Raster, Rect, Rgba8};
use snes_rom::Rom;

/// Thumbnail width in pixels (three screens at one quarter scale).
pub(crate) const WIDTH: u32 = 192;
/// Thumbnail height in pixels.
pub(crate) const HEIGHT: u32 = 108;
/// Source pixels per thumbnail pixel.
const SCALE: u32 = 4;

/// What the worker sends back.
struct Finished {
    id: LevelId,
    image: Option<egui::ColorImage>,
}

/// Thumbnail textures, filled in as the worker finishes them.
pub(crate) struct Thumbnails {
    requests: Sender<LevelId>,
    results: Receiver<Finished>,
    _thread: Option<JoinHandle<()>>,
    textures: HashMap<LevelId, egui::TextureHandle>,
    pending: HashSet<LevelId>,
    failed: HashSet<LevelId>,
}

impl std::fmt::Debug for Thumbnails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumbnails")
            .field("ready", &self.textures.len())
            .field("pending", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl Thumbnails {
    /// Spawns the worker over a copy of the ROM.
    pub(crate) fn start(ctx: &egui::Context, rom: &Rom, layout: &RomLayout) -> Self {
        let (requests, request_rx) = channel::<LevelId>();
        let (result_tx, results) = channel();
        let worker = Worker { rom: rom.clone(), layout: layout.clone(), ctx: ctx.clone() };
        let thread = std::thread::Builder::new()
            .name("thumbnails".to_owned())
            .spawn(move || worker.run(&request_rx, &result_tx))
            .ok();
        Self {
            requests,
            results,
            _thread: thread,
            textures: HashMap::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
        }
    }

    /// Asks for a level's thumbnail unless it is ready, queued or failed.
    pub(crate) fn request(&mut self, id: LevelId) {
        if self.textures.contains_key(&id)
            || self.pending.contains(&id)
            || self.failed.contains(&id)
        {
            return;
        }
        if self.requests.send(id).is_ok() {
            self.pending.insert(id);
        }
    }

    /// Uploads finished thumbnails as textures.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.results.try_recv() {
            self.pending.remove(&done.id);
            match done.image {
                Some(image) => {
                    let name = format!("thumb-{}", done.id);
                    let texture = ctx.load_texture(name, image, egui::TextureOptions::NEAREST);
                    self.textures.insert(done.id, texture);
                }
                None => {
                    self.failed.insert(done.id);
                }
            }
        }
    }

    /// The thumbnail of a level, once rendered.
    pub(crate) fn get(&self, id: LevelId) -> Option<&egui::TextureHandle> {
        self.textures.get(&id)
    }

    /// Whether a thumbnail could not be rendered.
    pub(crate) fn failed(&self, id: LevelId) -> bool {
        self.failed.contains(&id)
    }
}

struct Worker {
    rom: Rom,
    layout: RomLayout,
    ctx: egui::Context,
}

impl Worker {
    fn run(self, requests: &Receiver<LevelId>, results: &Sender<Finished>) {
        while let Ok(id) = requests.recv() {
            let image = self.render(id);
            if results.send(Finished { id, image }).is_err() {
                return;
            }
            self.ctx.request_repaint();
        }
    }

    fn render(&self, id: LevelId) -> Option<egui::ColorImage> {
        let scene = LevelScene::load(&self.rom, &self.layout, id).ok()?;
        let options = RenderOptions { layer1: true, layer2: true, sprites: false, owners: false };
        let full = scene.render(options);
        let crop = full.crop(Rect {
            x: 0,
            y: 0,
            w: WIDTH.saturating_mul(SCALE),
            h: HEIGHT.saturating_mul(SCALE),
        });
        Some(downscale(&crop))
    }
}

/// Box-filters a raster down by [`SCALE`] into a [`WIDTH`]×[`HEIGHT`] image,
/// padding a narrower source (a vertical level) with its backdrop colour.
fn downscale(src: &Raster) -> egui::ColorImage {
    let backdrop = src.get(0, 0).unwrap_or([0, 0, 0, 255]);
    let mut pixels = Vec::with_capacity(usize::try_from(WIDTH.saturating_mul(HEIGHT)).unwrap_or(0));
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            pixels.push(average(src, x.saturating_mul(SCALE), y.saturating_mul(SCALE), backdrop));
        }
    }
    let size = [usize::try_from(WIDTH).unwrap_or(0), usize::try_from(HEIGHT).unwrap_or(0)];
    let colors = pixels
        .into_iter()
        .map(|[r, g, b, a]| egui::Color32::from_rgba_unmultiplied(r, g, b, a))
        .collect();
    egui::ColorImage::new(size, colors)
}

/// The mean of the `SCALE`×`SCALE` block at `(x0, y0)`, treating pixels
/// outside the raster as `fill`.
fn average(src: &Raster, x0: u32, y0: u32, fill: Rgba8) -> Rgba8 {
    let mut sum = [0u32; 4];
    for dy in 0..SCALE {
        for dx in 0..SCALE {
            let p = src.get(x0.saturating_add(dx), y0.saturating_add(dy)).unwrap_or(fill);
            for (acc, channel) in sum.iter_mut().zip(p) {
                *acc = acc.saturating_add(u32::from(channel));
            }
        }
    }
    let n = SCALE.saturating_mul(SCALE).max(1);
    let mut out = [0u8; 4];
    for (o, s) in out.iter_mut().zip(sum) {
        *o = u8::try_from(s.checked_div(n).unwrap_or(0)).unwrap_or(u8::MAX);
    }
    out
}

#[cfg(test)]
mod tests {
    use smw_render::Raster;

    use super::downscale;

    #[test]
    fn downscale_averages_and_pads() {
        let src = Raster::filled(256, 432, [100, 0, 0, 255]);
        let image = downscale(&src);
        assert_eq!(image.size, [192, 108]);
        assert_eq!(image.pixels[0], egui::Color32::from_rgba_unmultiplied(100, 0, 0, 255));
        // Past the 256-pixel source width the picture is padded with the backdrop.
        assert_eq!(image.pixels[191], image.pixels[0]);
    }
}
