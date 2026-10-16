//! Which levels the list shows, in what order: the search box's terms, the
//! filter chips and the sort choice applied to the catalogue.

use smw_data::LevelId;
use smw_data::level::{Catalogue, LevelSummary};

use super::{LevelsView, Scope, Sort};
use crate::names::LevelNames;

/// A number facet a search term can test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Facet {
    Tileset,
    SpriteSet,
    Screens,
    Exits,
    Objects,
    Sprites,
}

impl Facet {
    fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "tileset" | "t" | "ts" => Self::Tileset,
            "spriteset" | "sp" | "ss" => Self::SpriteSet,
            "screens" | "scr" | "s" => Self::Screens,
            "exits" | "exit" | "x" => Self::Exits,
            "objects" | "obj" | "o" => Self::Objects,
            "sprites" | "spr" => Self::Sprites,
            _ => return None,
        })
    }

    fn of(self, level: &LevelSummary) -> usize {
        match self {
            Self::Tileset => usize::from(level.tileset),
            Self::SpriteSet => usize::from(level.sprite_set),
            Self::Screens => usize::from(level.screens),
            Self::Exits => level.exits.len(),
            Self::Objects => level.objects,
            Self::Sprites => level.sprites,
        }
    }
}

/// How a facet term compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Gt,
    Lt,
    Ge,
    Le,
}

impl Op {
    const fn holds(self, actual: usize, wanted: usize) -> bool {
        match self {
            Self::Eq => actual == wanted,
            Self::Gt => actual > wanted,
            Self::Lt => actual < wanted,
            Self::Ge => actual >= wanted,
            Self::Le => actual <= wanted,
        }
    }
}

/// One whitespace-separated word of the search box.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    /// Matched against the level number and name, case-insensitively.
    Text(String),
    /// `tileset:7`, `exits>0`, `screens>=10`, ...
    Facet(Facet, Op, usize),
}

/// Splits `word` into `key`, operator and number, if it has that shape.
fn parse_facet(word: &str) -> Option<Term> {
    const OPS: [(&str, Op); 6] = [
        (">=", Op::Ge),
        ("<=", Op::Le),
        (":", Op::Eq),
        ("=", Op::Eq),
        (">", Op::Gt),
        ("<", Op::Lt),
    ];
    for (symbol, op) in OPS {
        if let Some((key, value)) = word.split_once(symbol) {
            let facet = Facet::parse(&key.to_ascii_lowercase())?;
            let number = value
                .strip_prefix("0x")
                .map_or_else(|| value.parse().ok(), |hex| usize::from_str_radix(hex, 16).ok())?;
            return Some(Term::Facet(facet, op, number));
        }
    }
    None
}

fn parse(query: &str) -> Vec<Term> {
    query
        .split_whitespace()
        .map(|word| parse_facet(word).unwrap_or_else(|| Term::Text(word.to_ascii_uppercase())))
        .collect()
}

fn term_matches(term: &Term, level: &LevelSummary, name: Option<&str>) -> bool {
    match term {
        Term::Text(text) => {
            level.id.to_string().contains(text)
                || name.is_some_and(|n| n.to_ascii_uppercase().contains(text))
        }
        Term::Facet(facet, op, wanted) => op.holds(facet.of(level), *wanted),
    }
}

/// What the filter needs to know besides the catalogue.
pub(super) struct Filter<'a> {
    pub(super) catalogue: &'a Catalogue,
    pub(super) names: &'a LevelNames,
    pub(super) view: &'a LevelsView,
    /// The open level, when it has unsaved edits.
    pub(super) modified: Option<LevelId>,
}

impl Filter<'_> {
    /// The levels that pass the search and chips, in the chosen order.
    pub(super) fn visible(&self) -> Vec<LevelId> {
        let terms = parse(&self.view.query);
        let mut ids: Vec<LevelId> = self
            .catalogue
            .iter()
            .filter(|level| self.passes(level, &terms))
            .map(|level| level.id)
            .collect();
        self.sort(&mut ids);
        ids
    }

    fn passes(&self, level: &LevelSummary, terms: &[Term]) -> bool {
        let id = level.id;
        let in_scope = match self.view.scope {
            Scope::All => true,
            Scope::Overworld => id.is_overworld(),
            Scope::Sublevels => !id.is_overworld(),
        };
        in_scope
            && (!self.view.modified_only || self.modified == Some(id))
            && (!self.view.hide_unused || !self.catalogue.is_unused(id))
            && self.view.tileset.is_none_or(|t| t == level.tileset)
            && terms.iter().all(|term| term_matches(term, level, self.names.get(id)))
    }

    fn sort(&self, ids: &mut [LevelId]) {
        let summary = |id: &LevelId| self.catalogue.get(*id);
        match self.view.sort {
            Sort::Number => {}
            Sort::Name => ids.sort_by_key(|id| {
                let name = self.names.get(*id).map(str::to_ascii_uppercase);
                (name.is_none(), name, *id)
            }),
            Sort::Screens => {
                ids.sort_by_key(|id| {
                    (std::cmp::Reverse(summary(id).map_or(0, |l| l.screens)), *id)
                });
            }
            Sort::Tileset => ids.sort_by_key(|id| (summary(id).map_or(0, |l| l.tileset), *id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Facet, Op, Term, parse};

    #[test]
    fn parses_text_and_facets() {
        assert_eq!(
            parse("yoshi tileset:7 exits>0 scr>=0x10"),
            vec![
                Term::Text("YOSHI".to_owned()),
                Term::Facet(Facet::Tileset, Op::Eq, 7),
                Term::Facet(Facet::Exits, Op::Gt, 0),
                Term::Facet(Facet::Screens, Op::Ge, 16),
            ]
        );
        // An unknown key or a non-number stays a text term.
        assert_eq!(parse("foo:7"), vec![Term::Text("FOO:7".to_owned())]);
        assert_eq!(parse("t:x"), vec![Term::Text("T:X".to_owned())]);
    }
}
