//! A summary of every level slot: what the level list shows without loading
//! each level again, plus the exit graph between slots.

use snes_rom::{Rom, SnesAddr};

use super::{Level, SecondaryExit};
use crate::{LevelId, RomLayout};

/// Slots whose layer 1 data is shared by at least this many other slots are
/// unused: vanilla points every empty slot at one blank level.
const UNUSED_SHARE_THRESHOLD: usize = 4;

/// One screen exit of a level, resolved to the slot it leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExitEdge {
    /// Screen the exit is on.
    pub screen: u8,
    /// Where it leads (through the secondary exit table when `secondary`).
    pub target: LevelId,
    /// Whether the exit goes through a secondary exit.
    pub secondary: bool,
}

/// A level that leads into another one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Parent {
    /// The level whose exit leads here.
    pub from: LevelId,
    /// Which of its screens the exit is on.
    pub screen: u8,
}

/// Facts about one slot that the level list shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelSummary {
    /// The slot.
    pub id: LevelId,
    /// Where its layer 1 data lives.
    pub layer1: SnesAddr,
    /// Screens in the level.
    pub screens: u8,
    /// Whether the level scrolls vertically.
    pub vertical: bool,
    /// FG/BG tileset number.
    pub tileset: u8,
    /// Sprite set number.
    pub sprite_set: u8,
    /// Layer 1 objects (screen jumps and exits included).
    pub objects: usize,
    /// Sprites.
    pub sprites: usize,
    /// Screen exits, resolved.
    pub exits: Vec<ExitEdge>,
    /// Why the level failed to parse, if it did.
    pub error: Option<String>,
}

/// Every slot's summary, with the exit graph between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalogue {
    levels: Vec<LevelSummary>,
    parents: Vec<Vec<Parent>>,
    unused: Vec<bool>,
}

impl Catalogue {
    /// Parses every level and resolves its exits.
    #[must_use]
    pub fn scan(rom: &Rom, layout: &RomLayout) -> Self {
        let secondary = SecondaryExit::load_all(rom, layout).unwrap_or_default();
        let levels: Vec<LevelSummary> =
            LevelId::all().map(|id| summarize(rom, layout, &secondary, id)).collect();
        let mut this = Self {
            levels,
            parents: vec![Vec::new(); LevelId::COUNT],
            unused: vec![false; LevelId::COUNT],
        };
        this.rebuild_graph();
        this
    }

    /// Re-reads one slot (after it was saved) and updates the graph.
    pub fn refresh(&mut self, rom: &Rom, layout: &RomLayout, id: LevelId) {
        let secondary = SecondaryExit::load_all(rom, layout).unwrap_or_default();
        let summary = summarize(rom, layout, &secondary, id);
        if let Some(slot) = self.levels.get_mut(id.index()) {
            *slot = summary;
        }
        self.rebuild_graph();
    }

    /// The summary of a slot.
    #[must_use]
    pub fn get(&self, id: LevelId) -> Option<&LevelSummary> {
        self.levels.get(id.index())
    }

    /// Every slot in order.
    pub fn iter(&self) -> impl Iterator<Item = &LevelSummary> {
        self.levels.iter()
    }

    /// Levels whose exits lead to `id`.
    #[must_use]
    pub fn parents(&self, id: LevelId) -> &[Parent] {
        self.parents.get(id.index()).map_or(&[], Vec::as_slice)
    }

    /// Whether the slot holds the shared blank level.
    #[must_use]
    pub fn is_unused(&self, id: LevelId) -> bool {
        self.unused.get(id.index()).copied().unwrap_or(false)
    }

    fn rebuild_graph(&mut self) {
        let mut parents = vec![Vec::new(); LevelId::COUNT];
        for level in &self.levels {
            for exit in &level.exits {
                if let Some(list) = parents.get_mut(exit.target.index()) {
                    list.push(Parent { from: level.id, screen: exit.screen });
                }
            }
        }
        self.parents = parents;
        self.unused = self
            .levels
            .iter()
            .map(|level| {
                let sharers = self.levels.iter().filter(|o| o.layer1 == level.layer1).count();
                sharers > UNUSED_SHARE_THRESHOLD
            })
            .collect();
    }
}

fn summarize(
    rom: &Rom,
    layout: &RomLayout,
    secondary: &[SecondaryExit],
    id: LevelId,
) -> LevelSummary {
    match Level::load(rom, layout, id) {
        Ok(level) => {
            let exits = level
                .screen_exits()
                .iter()
                .filter_map(|(&screen, info)| {
                    let target = if info.secondary {
                        secondary.get(usize::from(info.destination))?.destination(layout)
                    } else {
                        info.destination
                    };
                    let target = LevelId::new(target)?;
                    Some(ExitEdge { screen, target, secondary: info.secondary })
                })
                .collect();
            LevelSummary {
                id,
                layer1: level.addresses.layer1,
                screens: level.header.screens(),
                vertical: level.vertical,
                tileset: level.header.tileset(),
                sprite_set: level.header.sprite_set(),
                objects: level.layer1.objects.len(),
                sprites: level.sprites.sprites.len(),
                exits,
                error: None,
            }
        }
        Err(err) => LevelSummary {
            id,
            layer1: super::LevelAddresses::load(rom, layout, id)
                .map_or(SnesAddr::from_parts(0, 0), |a| a.layer1),
            screens: 0,
            vertical: false,
            tileset: 0,
            sprite_set: 0,
            objects: 0,
            sprites: 0,
            exits: Vec::new(),
            error: Some(err.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::Catalogue;
    use crate::{LevelId, RomLayout};

    #[test]
    fn real_rom_catalogue() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).unwrap();
        let layout = RomLayout::detect(&rom).unwrap();
        let started = std::time::Instant::now();
        let catalogue = Catalogue::scan(&rom, &layout);
        eprintln!("scanned 512 levels in {:?}", started.elapsed());
        let id = |n| LevelId::new(n).unwrap();
        let yi1 = catalogue.get(id(0x105)).unwrap();
        assert_eq!(yi1.screens, 0x14);
        assert_eq!(yi1.tileset, 7);
        assert_eq!(yi1.exits.len(), 1);
        assert_eq!(yi1.exits[0].target, id(0x0CB));
        assert_eq!(yi1.exits[0].screen, 7);
        let parents: Vec<_> = catalogue.parents(id(0x0CB)).iter().map(|p| p.from).collect();
        assert!(parents.contains(&id(0x105)), "{parents:?}");
        assert!(parents.contains(&id(0x002)), "{parents:?}");
        assert!(catalogue.is_unused(id(0x025)));
        assert!(catalogue.is_unused(id(0x150)));
        assert!(!catalogue.is_unused(id(0x105)));
        assert!(!catalogue.is_unused(id(0x000)));
        assert!(catalogue.iter().all(|l| l.error.is_none()));
    }
}
