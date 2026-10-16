//! What the picker offers: every placeable object and sprite, with display
//! names.
//!
//! Names are labels from the community object and sprite lists (the ones
//! Lunar Magic shows), not ROM data; the numbers are what gets written. An
//! object the loader draws nothing for in the current tileset is dropped by
//! the thumbnail pass, so the list stays honest per tileset.

use smw_data::level::ObjectFormat;

use crate::edit::Tool;

/// Something the picker can place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PickItem {
    /// A standard (`number` 0x01–0x3F, `third` = size) or extended
    /// (`number` 0, `third` = extended number) object.
    Object {
        /// Object number.
        number: u8,
        /// Size byte or extended object number.
        third: u8,
    },
    /// A sprite.
    Sprite {
        /// Sprite number.
        number: u8,
    },
}

impl PickItem {
    /// The level-view tool that places this item.
    pub(crate) const fn tool(self) -> Tool {
        match self {
            Self::Object { number, third } => Tool::PlaceObject { number, third },
            Self::Sprite { number } => Tool::PlaceSprite { number, extra_bits: 0 },
        }
    }

    /// Whether `tool` places this item (any size for standard objects, any
    /// extra bits for sprites).
    pub(crate) const fn matches(self, tool: Tool) -> bool {
        match (self, tool) {
            (Self::Object { number: 0, third: a }, Tool::PlaceObject { number: 0, third: b }) => {
                a == b
            }
            (Self::Object { number: a, .. }, Tool::PlaceObject { number: b, .. })
            | (Self::Sprite { number: a }, Tool::PlaceSprite { number: b, .. }) => a == b,
            _ => false,
        }
    }

    /// Short id, e.g. `std 0F`, `ext 30`, `spr 1C`.
    pub(crate) fn id_label(self) -> String {
        match self {
            Self::Object { number: 0, third } => format!("ext {third:02X}"),
            Self::Object { number, .. } => format!("std {number:02X}"),
            Self::Sprite { number } => format!("spr {number:02X}"),
        }
    }
}

/// Picker groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Category {
    /// Standard objects shared by every tileset (0x01–0x2D).
    Standard,
    /// Standard objects whose look depends on the tileset (0x2E–0x3F).
    Tileset,
    /// Extended objects.
    Extended,
    /// Sprites.
    Sprite,
}

impl Category {
    /// Every category, in display order.
    pub(crate) const ALL: [Self; 4] = [Self::Standard, Self::Tileset, Self::Extended, Self::Sprite];

    /// Display name.
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Standard => "Standard objects",
            Self::Tileset => "Tileset objects",
            Self::Extended => "Extended objects",
            Self::Sprite => "Sprites",
        }
    }

    /// Whether this is an object category (not sprites).
    pub(crate) const fn is_object(self) -> bool {
        !matches!(self, Self::Sprite)
    }
}

/// One catalog entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogEntry {
    /// What it places.
    pub(crate) item: PickItem,
    /// Display name.
    pub(crate) name: String,
    /// Group.
    pub(crate) category: Category,
}

/// Size byte thumbnails and placement start with: 2×2 for objects that read
/// both nibbles, 2 long for the rest. Objects that overflow their slot at
/// this size fall back to [`FALLBACK_SIZE`].
pub(crate) const DEFAULT_SIZE: u8 = 0x11;
/// The smallest size, for objects too big at [`DEFAULT_SIZE`].
pub(crate) const FALLBACK_SIZE: u8 = 0x00;

/// First extended object with a routine (0x02–0x0F are null; 0x00 and 0x01
/// are screen exits and jumps, edited in the Exits panel).
const FIRST_EXTENDED: u8 = 0x10;
/// Lunar Magic repurposes standard objects 0x22–0x2D (direct Map16 tiles,
/// settings), so the picker leaves them out on Lunar Magic ROMs.
const LM_STANDARD: core::ops::RangeInclusive<u8> = 0x22..=0x2D;
/// Highest normal sprite; 0xC9 and up are shooters, generators and scroll
/// commands with their own placement rules.
const LAST_SPRITE: u8 = 0xC8;

/// Every candidate for a ROM whose objects use `format`.
pub(crate) fn catalog(format: ObjectFormat) -> Vec<CatalogEntry> {
    let standard = (0x01..=0x3Fu8)
        .filter(|n| format == ObjectFormat::Vanilla || !LM_STANDARD.contains(n))
        .map(|number| {
            let item = PickItem::Object { number, third: DEFAULT_SIZE };
            let name = STANDARD_NAMES.get(usize::from(number)).copied().flatten();
            let category = if number < 0x2E { Category::Standard } else { Category::Tileset };
            let name = name.map_or_else(|| format!("Object {number:02X}"), str::to_owned);
            CatalogEntry { item, name, category }
        });
    let extended = (FIRST_EXTENDED..=0xFFu8).map(|third| CatalogEntry {
        item: PickItem::Object { number: 0, third },
        name: format!("Extended object {third:02X}"),
        category: Category::Extended,
    });
    let sprites = (0x00..=LAST_SPRITE).map(|number| {
        let name = SPRITE_NAMES.get(usize::from(number)).copied().unwrap_or("Sprite");
        CatalogEntry {
            item: PickItem::Sprite { number },
            name: name.to_owned(),
            category: Category::Sprite,
        }
    });
    standard.chain(extended).chain(sprites).collect()
}

/// Names of the shared standard objects, indexed by number.
const STANDARD_NAMES: [Option<&str>; 0x22] = [
    None,
    Some("Water (blue)"),
    Some("Invisible coin blocks"),
    Some("Invisible note blocks"),
    Some("Invisible POW coins"),
    Some("Coins"),
    Some("Walk-through dirt"),
    Some("Water (other color)"),
    Some("Note blocks"),
    Some("Turn blocks"),
    Some("Coin ? blocks"),
    Some("Throw blocks"),
    Some("Black piranha plants"),
    Some("Cement blocks"),
    Some("Brown blocks"),
    Some("Vertical pipes"),
    Some("Horizontal pipes"),
    Some("Bullet shooter"),
    Some("Slopes"),
    Some("Ledge edges"),
    Some("Ground ledge"),
    Some("Midway/goal point"),
    Some("Blue coins"),
    Some("Rope/clouds"),
    Some("Water surface (animated)"),
    Some("Water surface"),
    Some("Lava surface"),
    Some("Net top edge"),
    Some("Donut bridge"),
    Some("Net bottom edge"),
    Some("Net vertical edge"),
    Some("Vertical pipe/bone/log"),
    Some("Horizontal pipe/bone/log"),
    Some("Long ground ledge"),
];

/// Sprite names, indexed by number.
const SPRITE_NAMES: [&str; 0xC9] = [
    "Green Koopa, no shell",
    "Red Koopa, no shell",
    "Blue Koopa, no shell",
    "Yellow Koopa, no shell",
    "Green Koopa",
    "Red Koopa",
    "Blue Koopa",
    "Yellow Koopa",
    "Green Koopa, flying left",
    "Green bouncing Koopa",
    "Red vertical flying Koopa",
    "Red horizontal flying Koopa",
    "Yellow Koopa with wings",
    "Bob-omb",
    "Keyhole",
    "Goomba",
    "Bouncing winged Goomba",
    "Buzzy Beetle",
    "Unused",
    "Spiny",
    "Spiny falling",
    "Fish, horizontal",
    "Fish, vertical",
    "Fish, generated",
    "Surface jumping fish",
    "Display message 1",
    "Classic Piranha Plant",
    "Bouncing football",
    "Bullet Bill",
    "Hopping flame",
    "Lakitu",
    "Magikoopa",
    "Magikoopa's magic",
    "Moving coin",
    "Green vertical net Koopa",
    "Red vertical net Koopa",
    "Green horizontal net Koopa",
    "Red horizontal net Koopa",
    "Thwomp",
    "Thwimp",
    "Big Boo",
    "Koopa Kid",
    "Upside-down Piranha Plant",
    "Sumo Brother's lightning",
    "Yoshi egg",
    "Baby Yoshi",
    "Spike Top",
    "Portable springboard",
    "Dry Bones, throws bones",
    "Bony Beetle",
    "Dry Bones, stays on ledge",
    "Podoboo",
    "Boss fireball",
    "Yoshi",
    "Unused",
    "Boo",
    "Eerie",
    "Eerie, wave motion",
    "Urchin, fixed",
    "Urchin, wall detect",
    "Urchin, wall follow",
    "Rip Van Fish",
    "POW",
    "Para-Goomba",
    "Para-Bomb",
    "Dolphin, horizontal",
    "Dolphin, horizontal 2",
    "Dolphin, vertical",
    "Torpedo Ted",
    "Directional coins",
    "Diggin' Chuck",
    "Swimming/jumping fish",
    "Diggin' Chuck's rock",
    "Growing/shrinking pipe end",
    "Goal point question sphere",
    "Pipe-dwelling Lakitu",
    "Exploding block",
    "Ground-dwelling Monty Mole",
    "Ledge-dwelling Monty Mole",
    "Jumping Piranha Plant",
    "Jumping fire-spitting Piranha Plant",
    "Ninji",
    "Moving ledge hole",
    "Throw block",
    "Climbing net door",
    "Checkerboard platform, horizontal",
    "Flying rock platform, horizontal",
    "Checkerboard platform, vertical",
    "Flying rock platform, vertical",
    "Turn block bridge, horizontal and vertical",
    "Turn block bridge, horizontal",
    "Brown platform, floating",
    "Checkerboard platform, floating",
    "Orange platform, floating",
    "Orange platform, goes on forever",
    "Brown platform on chain",
    "Flat switch palace switch",
    "Floating skulls",
    "Brown platform, line-guided",
    "Checker/brown platform, line-guided",
    "Rope mechanism, line-guided",
    "Chainsaw, line-guided",
    "Upside-down chainsaw, line-guided",
    "Grinder, line-guided",
    "Fuzz ball, line-guided",
    "Unused",
    "Coin game cloud",
    "Spring board, left wall",
    "Spring board, right wall",
    "Invisible solid block",
    "Dino Rhino",
    "Dino Torch",
    "Pokey",
    "Super Koopa, red cape",
    "Super Koopa, yellow cape",
    "Super Koopa, feather",
    "Mushroom",
    "Flower",
    "Star",
    "Feather",
    "1-Up mushroom",
    "Growing vine",
    "Firework",
    "Goal tape",
    "Princess Peach",
    "P-Balloon",
    "Flying red coin",
    "Flying yellow 1-Up",
    "Key",
    "Changing item",
    "Bonus game",
    "Left flying question block",
    "Flying question block",
    "Unused",
    "Wiggler",
    "Lakitu's cloud",
    "Unused (winged cage)",
    "Layer 3 smash",
    "Bird from Yoshi's house",
    "Puff of smoke from Yoshi's house",
    "Fireplace smoke",
    "Ghost house exit sign and door",
    "Invisible warp hole blocks",
    "Scale platforms",
    "Large green gas bubble",
    "Chargin' Chuck",
    "Splittin' Chuck",
    "Bouncin' Chuck",
    "Whistlin' Chuck",
    "Clappin' Chuck",
    "Unused (Chargin' Chuck)",
    "Puntin' Chuck",
    "Pitchin' Chuck",
    "Volcano Lotus",
    "Sumo Brother",
    "Hammer Brother",
    "Flying blocks for Hammer Brother",
    "Bubble with sprite",
    "Ball and Chain",
    "Banzai Bill",
    "Activates Bowser scene",
    "Bowser's bowling ball",
    "Mecha-Koopa",
    "Grey platform on chain",
    "Floating spike ball",
    "Fuzzball/Sparky, ground-guided",
    "HotHead, ground-guided",
    "Iggy's ball",
    "Blargg",
    "Reznor",
    "Fishbone",
    "Rex",
    "Wooden spike, moving down and up",
    "Wooden spike, moving up and down",
    "Fishin' Boo",
    "Boo Block",
    "Reflecting stream of Boo Buddies",
    "Creating/eating block",
    "Falling spike",
    "Bowser statue fireball",
    "Grinder, non-line-guided",
    "Sinking fireball",
    "Reflecting fireball",
    "Carrot top lift, upper right",
    "Carrot top lift, upper left",
    "Info box",
    "Timed lift",
    "Grey moving castle block",
    "Bowser statue",
    "Sliding blue Koopa",
    "Swooper bat",
    "Mega Mole",
    "Grey platform on lava",
    "Flying grey turn blocks",
    "Blurp fish",
    "Porcu-Puffer fish",
    "Grey platform that falls",
    "Big Boo boss",
    "Dark room with spotlight",
    "Invisible mushroom",
    "Light switch block for dark room",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_each_kind_once() {
        let vanilla = catalog(ObjectFormat::Vanilla);
        let count = |c: Category| vanilla.iter().filter(|e| e.category == c).count();
        assert_eq!(count(Category::Standard), 0x2D);
        assert_eq!(count(Category::Tileset), 0x12);
        assert_eq!(count(Category::Extended), 0xF0);
        assert_eq!(count(Category::Sprite), 0xC9);
        let lm = catalog(ObjectFormat::LunarMagic);
        assert_eq!(vanilla.len().saturating_sub(lm.len()), 12);
        let names: Vec<_> = vanilla.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Vertical pipes") && names.contains(&"Bullet Bill"));
    }

    #[test]
    fn tools_round_trip() {
        let pipe = PickItem::Object { number: 0x0F, third: 0x11 };
        assert!(pipe.matches(Tool::PlaceObject { number: 0x0F, third: 0x35 }));
        let block = PickItem::Object { number: 0, third: 0x30 };
        assert!(!block.matches(Tool::PlaceObject { number: 0, third: 0x31 }));
        assert!(block.matches(block.tool()));
        let koopa = PickItem::Sprite { number: 0x04 };
        assert!(koopa.matches(Tool::PlaceSprite { number: 0x04, extra_bits: 1 }));
        assert!(!koopa.matches(Tool::Select));
        assert_eq!(koopa.id_label(), "spr 04");
    }
}
