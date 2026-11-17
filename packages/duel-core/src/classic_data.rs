//! Classic identity mode content, transcribed from the 2008 standard group and base deck.
//! Presentation names and skill text are data. The runtime never loads upstream JavaScript.
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct General {
    pub id: String,
    pub name: String,
    pub faction: String,
    pub gender: String,
    pub max_hp: u16,
    pub is_lord: bool,
    pub skills: Vec<SkillDef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Suit {
    Spade,
    Club,
    Heart,
    Diamond,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PhysicalCard {
    pub id: u32,
    pub kind: String,
    pub suit: Suit,
    pub rank: u8,
}

pub fn roster() -> &'static [General] {
    static GENERALS: OnceLock<Vec<General>> = OnceLock::new();
    GENERALS.get_or_init(|| {
        serde_json::from_str(include_str!("../data/standard-2008-generals.json"))
            .expect("invalid bundled standard general data")
    })
}

pub fn standard_deck() -> Vec<PhysicalCard> {
    static DECK: OnceLock<Vec<PhysicalCard>> = OnceLock::new();
    DECK.get_or_init(|| {
        serde_json::from_str(include_str!("../data/standard-deck.json"))
            .expect("invalid bundled standard card data")
    })
    .clone()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Lord,
    Loyalist,
    Rebel,
    Renegade,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Court,
    Rebels,
    Renegade,
}

/// The configured ordinary identity tables from game/config.json (2–10 players).
pub fn deal_roles(count: usize) -> Option<Vec<Role>> {
    let (loyalists, renegades) = match count {
        2 => (0, 0),
        3 => (0, 1),
        4..=6 => (1, 1),
        7..=8 => (2, 1),
        9 => (3, 1),
        10 => (3, 2),
        _ => return None,
    };
    let mut roles = vec![Role::Lord];
    roles.resize(1 + loyalists, Role::Loyalist);
    roles.resize(1 + loyalists + renegades, Role::Renegade);
    roles.resize(count, Role::Rebel);
    Some(roles)
}

pub fn winning_side(alive: &[Role]) -> Option<Side> {
    if alive.is_empty() {
        return None;
    }
    if alive.contains(&Role::Lord) {
        return (!alive
            .iter()
            .any(|role| matches!(role, Role::Renegade | Role::Rebel)))
        .then_some(Side::Court);
    }
    Some(if alive == [Role::Renegade] {
        Side::Renegade
    } else {
        Side::Rebels
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn complete_2008_group_and_physical_deck() {
        let generals = roster();
        assert_eq!(generals.len(), 25);
        assert_eq!(
            generals.iter().map(|g| &g.id).collect::<HashSet<_>>().len(),
            25
        );
        assert!(generals
            .iter()
            .all(|g| !g.skills.is_empty() && (3..=4).contains(&g.max_hp)));
        let cards = standard_deck();
        assert_eq!(cards.len(), 104);
        for (i, card) in cards.iter().enumerate() {
            assert_eq!(card.id, i as u32);
            assert!((1..=13).contains(&card.rank));
        }
        for suit in [Suit::Spade, Suit::Club, Suit::Heart, Suit::Diamond] {
            assert_eq!(cards.iter().filter(|c| c.suit == suit).count(), 26);
        }
        assert_eq!(
            cards.iter().map(|c| &c.kind).collect::<HashSet<_>>().len(),
            30
        );
    }

    #[test]
    fn identity_distributions_and_victory() {
        use Role::*;
        for (index, (loyal, rebel, lone)) in [
            (0, 1, 0),
            (0, 1, 1),
            (1, 1, 1),
            (1, 2, 1),
            (1, 3, 1),
            (2, 3, 1),
            (2, 4, 1),
            (3, 4, 1),
            (3, 4, 2),
        ]
        .iter()
        .enumerate()
        {
            let roles = deal_roles(index + 2).unwrap();
            let count = |r: Role| roles.iter().filter(|&&x| x == r).count();
            assert_eq!(
                (count(Lord), count(Loyalist), count(Rebel), count(Renegade)),
                (1, *loyal, *rebel, *lone)
            );
        }
        assert!(deal_roles(11).is_none());
        assert_eq!(winning_side(&[Lord, Renegade]), None);
        assert_eq!(winning_side(&[Lord, Loyalist]), Some(Side::Court));
        assert_eq!(winning_side(&[Renegade]), Some(Side::Renegade));
        assert_eq!(winning_side(&[Renegade, Loyalist]), Some(Side::Rebels));
    }
}
