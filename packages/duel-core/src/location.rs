use crate::{Card, CardId, Game, SeatId, Zone};

pub(crate) const HELD_ZONES: [Zone; 4] =
    [Zone::Hand, Zone::Equipment, Zone::Judgment, Zone::Expansion];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardZone {
    Hand,
    Equipment,
    Judgment,
    Expansion,
    DrawPile,
    Ordering,
    Special,
    DiscardPile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardLocation {
    /// A newly drawn judgment card can already have an owner before entering its physical zone.
    pub zone: Option<CardZone>,
    pub owner: Option<SeatId>,
}

impl Game {
    pub(crate) fn held_card_location(&self, id: CardId) -> Option<(SeatId, Zone, Card)> {
        self.seats.iter().find_map(|player| {
            HELD_ZONES.into_iter().find_map(|zone| {
                player
                    .zone(zone)
                    .iter()
                    .find(|card| card.id == id)
                    .map(|&card| (player.id, zone, card))
            })
        })
    }

    pub(crate) fn take_unowned_card(&mut self, card: Card) -> Option<Card> {
        if self.resolving.contains(&card) {
            return Some(self.take_resolving(card.id));
        }
        [&mut self.transferring, &mut self.discard]
            .into_iter()
            .find(|pile| pile.contains(&card))
            .map(|pile| Self::remove_card(pile, card.id))
            .or_else(|| {
                let deck = &mut self.classic.as_mut()?.deck;
                deck.contains(&card)
                    .then(|| Self::remove_card(deck, card.id))
            })
    }

    /// A hidden hand loss obscures old knowledge of the remaining cards.
    pub(crate) fn obscure_remaining_hand(&mut self, owner: SeatId) {
        if !self.known_to_all.is_empty() || !self.known_to_seats.is_empty() {
            let player = self
                .seats
                .iter()
                .find(|p| p.id == owner)
                .expect("validated payer");
            for held in &player.hand {
                self.known_to_all.remove(&held.id);
                self.known_to_seats.remove(&held.id);
            }
        }
    }

    /// Locate an actual physical card for trusted rules. Seat views handle disclosure separately.
    pub fn card_location(&self, id: CardId) -> Option<CardLocation> {
        if let Some((owner, zone, _)) = self.held_card_location(id) {
            return Some(CardLocation {
                zone: Some(match zone {
                    Zone::Hand => CardZone::Hand,
                    Zone::Equipment => CardZone::Equipment,
                    Zone::Judgment => CardZone::Judgment,
                    Zone::Expansion => CardZone::Expansion,
                }),
                owner: Some(owner),
            });
        }
        let owner = self.judging_owner(id);
        let zone = [
            (CardZone::Ordering, self.resolving.as_slice()),
            (CardZone::Special, self.transferring.as_slice()),
            (CardZone::DiscardPile, self.discard.as_slice()),
            (
                CardZone::DrawPile,
                self.classic
                    .as_ref()
                    .map_or(&[][..], |table| table.deck.as_slice()),
            ),
        ]
        .into_iter()
        .find_map(|(zone, cards)| cards.iter().any(|card| card.id == id).then_some(zone));
        (zone.is_some() || owner.is_some()).then_some(CardLocation { zone, owner })
    }
}
