//! Level view camera: fit, zoom presets, recenter and screen jumps.
//!
//! The camera is the `egui::Scene` rectangle: the part of the level (in level
//! pixels) the panel shows. Everything here is plain geometry on that
//! rectangle and the panel's size, so it can be tested without a window.

use egui::{Event, InputState, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Vec2};

/// Zoom limits for the level view. Low enough that a 32-screen level fits.
pub(crate) const ZOOM_MIN: f32 = 0.05;
/// Upper zoom limit.
pub(crate) const ZOOM_MAX: f32 = 16.0;

/// Zoom steps for zoom in and out: integer scales above 1 keep pixels crisp.
const ZOOM_LADDER: [f32; 11] = [0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 16.0];

/// Screen-pixel gap left around fitted content.
const FIT_MARGIN: f32 = 12.0;

/// Pixels per screen along the level's main axis.
const SCREEN_PX: f32 = 256.0;

/// Something to do to the level view camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CameraCommand {
    /// Show every screen the level uses.
    FitLevel,
    /// Fill the panel across the level's short axis, at the level start.
    FitHeight,
    /// Show the selection.
    FitSelection,
    /// An integer scale, keeping the view centre.
    Zoom(u8),
    /// One step in (+1) or out (−1) on the zoom ladder.
    ZoomStep(i8),
    /// Centre the level without changing the zoom.
    Recenter,
    /// Put a screen at the left (or top) edge.
    GoToScreen(u8),
    /// Move by whole screens.
    ScreenStep(i8),
    /// The first screen.
    Start,
    /// The last screen.
    End,
}

impl CameraCommand {
    /// Menu and tooltip name.
    pub(crate) fn label(self) -> String {
        match self {
            Self::FitLevel => "Fit level".to_owned(),
            Self::FitHeight => "Fit height".to_owned(),
            Self::FitSelection => "Fit selection".to_owned(),
            Self::Zoom(z) => format!("Zoom {z}×"),
            Self::ZoomStep(s) if s > 0 => "Zoom in".to_owned(),
            Self::ZoomStep(_) => "Zoom out".to_owned(),
            Self::Recenter => "Recenter".to_owned(),
            Self::GoToScreen(s) => format!("Go to screen {s:X}"),
            Self::ScreenStep(s) if s > 0 => "Next screen".to_owned(),
            Self::ScreenStep(_) => "Previous screen".to_owned(),
            Self::Start => "Level start".to_owned(),
            Self::End => "Level end".to_owned(),
        }
    }
}

/// Keyboard shortcuts for the camera, in menu order. Keys also match
/// physically, so Shift+1 works on layouts where it types `!`.
pub(crate) const SHORTCUTS: [(KeyboardShortcut, CameraCommand); 14] = [
    (KeyboardShortcut::new(Modifiers::SHIFT, Key::Num1), CameraCommand::FitLevel),
    (KeyboardShortcut::new(Modifiers::SHIFT, Key::Num2), CameraCommand::FitSelection),
    (KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0), CameraCommand::FitHeight),
    (KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1), CameraCommand::Zoom(1)),
    (KeyboardShortcut::new(Modifiers::COMMAND, Key::Num2), CameraCommand::Zoom(2)),
    (KeyboardShortcut::new(Modifiers::COMMAND, Key::Num3), CameraCommand::Zoom(3)),
    (KeyboardShortcut::new(Modifiers::COMMAND, Key::Num4), CameraCommand::Zoom(4)),
    (KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals), CameraCommand::ZoomStep(1)),
    (KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus), CameraCommand::ZoomStep(-1)),
    (KeyboardShortcut::new(Modifiers::NONE, Key::F), CameraCommand::Recenter),
    (KeyboardShortcut::new(Modifiers::NONE, Key::PageUp), CameraCommand::ScreenStep(-1)),
    (KeyboardShortcut::new(Modifiers::NONE, Key::PageDown), CameraCommand::ScreenStep(1)),
    (KeyboardShortcut::new(Modifiers::NONE, Key::Home), CameraCommand::Start),
    (KeyboardShortcut::new(Modifiers::NONE, Key::End), CameraCommand::End),
];

/// Extra keys that do the same as a listed shortcut (keypad plus).
const ALIASES: [(KeyboardShortcut, CameraCommand); 1] =
    [(KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus), CameraCommand::ZoomStep(1))];

/// The shortcut for `command`, for menus and tooltips.
pub(crate) fn shortcut_for(command: CameraCommand) -> Option<&'static KeyboardShortcut> {
    SHORTCUTS.iter().find(|(_, c)| *c == command).map(|(s, _)| s)
}

/// Takes the first camera shortcut pressed this frame out of the input.
pub(crate) fn consume_shortcut(input: &mut InputState) -> Option<CameraCommand> {
    let mut found = None;
    input.events.retain(|event| {
        let Event::Key { key, physical_key, pressed: true, modifiers, .. } = event else {
            return true;
        };
        let hit = SHORTCUTS.iter().chain(&ALIASES).find(|(s, _)| {
            (s.logical_key == physical_key.unwrap_or(*key) || s.logical_key == *key)
                && modifiers.matches_logically(s.modifiers)
        });
        match hit {
            Some((_, command)) if found.is_none() => {
                found = Some(*command);
                false
            }
            _ => true,
        }
    });
    found
}

/// What the camera needs to know about the level.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LevelExtent {
    /// The screens the level uses, in scene coordinates.
    pub(crate) used: Rect,
    /// Screens run top to bottom.
    pub(crate) vertical: bool,
    /// Number of screens in use.
    pub(crate) screens: u8,
}

impl LevelExtent {
    /// The used part of a `width`×`height` picture at `origin`.
    #[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
    pub(crate) fn new(origin: Pos2, size: Vec2, vertical: bool, screens: u8) -> Self {
        let length = f32::from(screens) * SCREEN_PX;
        let used = if vertical {
            Vec2::new(size.x, length.min(size.y))
        } else {
            Vec2::new(length.min(size.x), size.y)
        };
        Self { used: Rect::from_min_size(origin, used), vertical, screens }
    }

    /// The scene coordinate where `screen` starts along the main axis.
    fn screen_start(&self, screen: u8) -> f32 {
        let along = f32::from(screen.min(self.screens.saturating_sub(1))) * SCREEN_PX;
        if self.vertical { self.used.min.y + along } else { self.used.min.x + along }
    }
}

/// Scale of `current` shown in a `viewport`-sized panel.
pub(crate) fn zoom_of(current: Rect, viewport: Vec2) -> f32 {
    if current.width() > 0.0 && current.height() > 0.0 {
        // `Scene` letterboxes: the smaller ratio is the one it uses.
        (viewport.x / current.width()).min(viewport.y / current.height())
    } else {
        1.0
    }
}

/// The camera rectangle centred on `center` at `zoom`.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn rect_at(center: Pos2, viewport: Vec2, zoom: f32) -> Rect {
    Rect::from_center_size(center, viewport / zoom.clamp(ZOOM_MIN, ZOOM_MAX))
}

/// The camera that shows all of `target` with a margin.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn fit(target: Rect, viewport: Vec2, max_zoom: f32) -> Rect {
    let room = (viewport - Vec2::splat(2.0 * FIT_MARGIN)).max(Vec2::splat(1.0));
    let size = target.size().max(Vec2::splat(1.0));
    let zoom = (room.x / size.x).min(room.y / size.y).min(max_zoom);
    rect_at(target.center(), viewport, zoom)
}

/// Moves `current` along the main axis so its leading edge is at `start`.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn scroll_to(current: Rect, start: f32, vertical: bool) -> Rect {
    let delta = if vertical {
        Vec2::new(0.0, start - current.min.y)
    } else {
        Vec2::new(start - current.min.x, 0.0)
    };
    current.translate(delta)
}

/// The next zoom on the ladder in the direction of `step`.
fn step_zoom(zoom: f32, step: i8) -> f32 {
    // A small tolerance so 1.999 steps up to 3, not 2.
    let eps = 1e-3;
    if step > 0 {
        ZOOM_LADDER.iter().copied().find(|z| *z > zoom + eps).unwrap_or(ZOOM_MAX)
    } else {
        ZOOM_LADDER.iter().rev().copied().find(|z| *z < zoom - eps).unwrap_or(ZOOM_MIN)
    }
}

/// The screen whose start is nearest the current view's leading edge.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "rounded, clamped float screen index"
)]
fn current_screen(current: Rect, extent: &LevelExtent) -> u8 {
    let edge = if extent.vertical {
        current.min.y - extent.used.min.y
    } else {
        current.min.x - extent.used.min.x
    };
    let max = f32::from(extent.screens.saturating_sub(1));
    (edge / SCREEN_PX).round().clamp(0.0, max) as u8
}

/// Applies a step of whole screens.
fn screen_step(current: Rect, extent: &LevelExtent, step: i8) -> Rect {
    let here = current_screen(current, extent);
    let to = if step < 0 {
        here.saturating_sub(step.unsigned_abs())
    } else {
        here.saturating_add(step.unsigned_abs())
    };
    scroll_to(current, extent.screen_start(to), extent.vertical)
}

/// The camera that fills the short axis and starts at the level start.
#[allow(clippy::arithmetic_side_effects, reason = "egui geometry math")]
fn fit_height(viewport: Vec2, extent: &LevelExtent) -> Rect {
    let used = extent.used;
    let zoom = if extent.vertical {
        viewport.x / used.width().max(1.0)
    } else {
        viewport.y / used.height().max(1.0)
    };
    let view = rect_at(used.center(), viewport, zoom);
    let start = if extent.vertical { used.min.y } else { used.min.x };
    scroll_to(view, start, extent.vertical)
}

/// The camera after `command`.
pub(crate) fn apply(
    command: CameraCommand,
    current: Rect,
    viewport: Vec2,
    extent: &LevelExtent,
    selection: Option<Rect>,
) -> Rect {
    let zoom = zoom_of(current, viewport);
    match command {
        CameraCommand::FitLevel => fit(extent.used, viewport, ZOOM_MAX),
        CameraCommand::FitHeight => fit_height(viewport, extent),
        CameraCommand::FitSelection => {
            selection.map_or(current, |s| fit(s.expand(32.0), viewport, 4.0))
        }
        CameraCommand::Zoom(z) => rect_at(current.center(), viewport, f32::from(z)),
        CameraCommand::ZoomStep(step) => rect_at(current.center(), viewport, step_zoom(zoom, step)),
        CameraCommand::Recenter => rect_at(extent.used.center(), viewport, zoom),
        CameraCommand::GoToScreen(s) => scroll_to(current, extent.screen_start(s), extent.vertical),
        CameraCommand::ScreenStep(step) => screen_step(current, extent, step),
        CameraCommand::Start => scroll_to(current, extent.screen_start(0), extent.vertical),
        CameraCommand::End => {
            let last = extent.screen_start(extent.screens.saturating_sub(1));
            let end = last + SCREEN_PX;
            let length = if extent.vertical { current.height() } else { current.width() };
            scroll_to(current, (end - length).max(extent.screen_start(0)), extent.vertical)
        }
    }
}

/// The screen at the view's leading edge, for the toolbar.
pub(crate) fn screen_at(current: Rect, extent: &LevelExtent) -> u8 {
    current_screen(current, extent)
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "exact results of simple float math")]
mod tests {
    use super::*;

    const VIEW: Vec2 = Vec2::new(1000.0, 600.0);

    fn horizontal() -> LevelExtent {
        LevelExtent::new(Pos2::ZERO, Vec2::new(8192.0, 432.0), false, 20)
    }

    fn vertical() -> LevelExtent {
        LevelExtent::new(Pos2::ZERO, Vec2::new(512.0, 8192.0), true, 14)
    }

    fn start() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 50.0), VIEW)
    }

    #[test]
    fn extent_covers_used_screens_only() {
        assert_eq!(horizontal().used, Rect::from_min_size(Pos2::ZERO, Vec2::new(5120.0, 432.0)));
        assert_eq!(vertical().used, Rect::from_min_size(Pos2::ZERO, Vec2::new(512.0, 3584.0)));
    }

    #[test]
    fn fit_level_contains_and_centers_the_level() {
        let extent = horizontal();
        let view = apply(CameraCommand::FitLevel, start(), VIEW, &extent, None);
        assert!(view.contains_rect(extent.used));
        assert!((view.center() - extent.used.center()).length() < 1e-3);
        let zoom = zoom_of(view, VIEW);
        assert!((zoom - (1000.0 - 24.0) / 5120.0).abs() < 1e-4);
    }

    #[test]
    fn integer_zoom_keeps_the_center() {
        let view = apply(CameraCommand::Zoom(2), start(), VIEW, &horizontal(), None);
        assert_eq!(view.center(), start().center());
        assert_eq!(view.width(), 500.0);
        assert_eq!(zoom_of(view, VIEW), 2.0);
    }

    #[test]
    fn zoom_steps_snap_to_the_ladder() {
        assert_eq!(step_zoom(1.37, 1), 2.0);
        assert_eq!(step_zoom(1.37, -1), 1.0);
        assert_eq!(step_zoom(2.0, 1), 3.0);
        assert_eq!(step_zoom(ZOOM_MAX, 1), ZOOM_MAX);
        assert_eq!(step_zoom(ZOOM_MIN, -1), ZOOM_MIN);
    }

    #[test]
    fn go_to_screen_moves_the_main_axis() {
        let h = apply(CameraCommand::GoToScreen(3), start(), VIEW, &horizontal(), None);
        assert_eq!(h.min, Pos2::new(768.0, 50.0));
        let v = apply(CameraCommand::GoToScreen(3), start(), VIEW, &vertical(), None);
        assert_eq!(v.min, Pos2::new(100.0, 768.0));
        let past = apply(CameraCommand::GoToScreen(0x1F), start(), VIEW, &horizontal(), None);
        assert_eq!(past.min.x, 19.0 * 256.0);
    }

    #[test]
    fn screen_steps_and_ends() {
        let extent = horizontal();
        let next = apply(CameraCommand::ScreenStep(1), start(), VIEW, &extent, None);
        assert_eq!(next.min.x, 256.0);
        let back = apply(CameraCommand::ScreenStep(-1), next, VIEW, &extent, None);
        assert_eq!(back.min.x, 0.0);
        let end = apply(CameraCommand::End, start(), VIEW, &extent, None);
        assert_eq!(end.max.x, 5120.0);
        assert_eq!(screen_at(end, &extent), 16);
    }

    #[test]
    fn fit_height_starts_at_the_level_start() {
        let view = apply(CameraCommand::FitHeight, start(), VIEW, &horizontal(), None);
        assert_eq!(view.min.x, 0.0);
        assert!((view.height() - 432.0).abs() < 1e-3);
        let tall = apply(CameraCommand::FitHeight, start(), VIEW, &vertical(), None);
        assert_eq!(tall.min.y, 0.0);
        assert!((tall.width() - 512.0).abs() < 1e-3);
    }

    #[test]
    fn fit_selection_without_selection_keeps_the_view() {
        let view = apply(CameraCommand::FitSelection, start(), VIEW, &horizontal(), None);
        assert_eq!(view, start());
        let tile = Rect::from_min_size(Pos2::new(320.0, 160.0), Vec2::splat(16.0));
        let framed = apply(CameraCommand::FitSelection, start(), VIEW, &horizontal(), Some(tile));
        assert!(framed.contains_rect(tile));
        assert_eq!(zoom_of(framed, VIEW), 4.0);
    }

    #[test]
    fn every_shortcut_is_distinct() {
        for (i, (a, _)) in SHORTCUTS.iter().enumerate() {
            for (b, _) in SHORTCUTS.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }
}
