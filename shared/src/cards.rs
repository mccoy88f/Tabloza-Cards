//! Mazzo di carte italiane da 40 carte (semi: Denari, Coppe, Spade, Bastoni).

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Suit {
    Denari,
    Coppe,
    Spade,
    Bastoni,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Denari, Suit::Coppe, Suit::Spade, Suit::Bastoni];

    pub fn name(self) -> &'static str {
        match self {
            Suit::Denari => "Denari",
            Suit::Coppe => "Coppe",
            Suit::Spade => "Spade",
            Suit::Bastoni => "Bastoni",
        }
    }

    /// Simbolo compatto usato dall'interfaccia.
    pub fn symbol(self) -> &'static str {
        match self {
            Suit::Denari => "🪙",
            Suit::Coppe => "🏆",
            Suit::Spade => "⚔️",
            Suit::Bastoni => "🪵",
        }
    }
}

/// Una carta: `rank` va da 1 (Asso) a 10 (Re). 8 = Fante, 9 = Cavallo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Card {
    pub suit: Suit,
    pub rank: u8,
}

impl Card {
    pub const fn new(suit: Suit, rank: u8) -> Self {
        Card { suit, rank }
    }

    pub fn rank_name(self) -> &'static str {
        match self.rank {
            1 => "Asso",
            2 => "Due",
            3 => "Tre",
            4 => "Quattro",
            5 => "Cinque",
            6 => "Sei",
            7 => "Sette",
            8 => "Fante",
            9 => "Cavallo",
            10 => "Re",
            _ => "?",
        }
    }

    /// Etichetta breve sulla carta (A, 2..7, F, C, R).
    pub fn short_label(self) -> String {
        match self.rank {
            1 => "A".into(),
            8 => "F".into(),
            9 => "C".into(),
            10 => "R".into(),
            n => n.to_string(),
        }
    }

    /// Valore della carta ai fini della primiera.
    pub fn primiera_value(self) -> u32 {
        match self.rank {
            7 => 21,
            6 => 18,
            1 => 16,
            5 => 15,
            4 => 14,
            3 => 13,
            2 => 12,
            _ => 10,
        }
    }

    pub fn is_settebello(self) -> bool {
        self.suit == Suit::Denari && self.rank == 7
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} di {}", self.rank_name(), self.suit.name())
    }
}

/// Mazzo completo ordinato di 40 carte.
pub fn full_deck() -> Vec<Card> {
    Suit::ALL
        .iter()
        .flat_map(|&s| (1..=10).map(move |r| Card::new(s, r)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_has_40_unique_cards() {
        let deck = full_deck();
        assert_eq!(deck.len(), 40);
        let set: std::collections::HashSet<_> = deck.iter().collect();
        assert_eq!(set.len(), 40);
    }
}
