mod lifecycle;
#[cfg(test)]
mod tests;

use crate::classic_data::{self, Role, Side};
use crate::{
    Action, Card, CardFace, CardId, CardKind, ChoiceKind, Command, Decision, EventKind, Game,
    Program, RuleError, Seat, SeatId, TurnView,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MatchResult {
    Winner(Side),
    Draw,
}

impl MatchResult {
    pub(super) fn winner(self) -> Option<Side> {
        match self {
            Self::Winner(side) => Some(side),
            Self::Draw => None,
        }
    }
}

pub(super) struct Classic {
    pub(super) human: SeatId,
    lord: SeatId,
    pub(super) deck: Vec<Card>,
    pub(super) faces: Vec<CardFace>,
    random: TableRandom,
    pub(super) turn: Option<TurnView>,
    sha_played: bool,
    pub(super) result: Option<MatchResult>,
    result_announced: bool,
}

struct TableRandom(u64);

struct PlannedUse {
    targets: Vec<SeatId>,
    program: Program,
}

impl TableRandom {
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for index in (1..items.len()).rev() {
            items.swap(index, (self.next() % (index as u64 + 1)) as usize);
        }
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }
}

impl Game {
    /// Start an ordinary classic identity table at the lord's first play phase.
    pub fn classic(
        seed: u32,
        human: SeatId,
        hero_id: &str,
        count: usize,
    ) -> Result<Self, RuleError> {
        let mut roles =
            classic_data::deal_roles(count).ok_or(RuleError::UnsupportedSeatCount { count })?;
        if human as usize >= count {
            return Err(RuleError::UnknownSeat { seat: human });
        }
        let roster = classic_data::roster();
        let hero = roster
            .iter()
            .find(|hero| hero.id == hero_id)
            .ok_or(RuleError::UnknownHero)?;

        let mut random = TableRandom(u64::from(seed));
        random.shuffle(&mut roles);
        let lord = roles
            .iter()
            .position(|role| *role == Role::Lord)
            .expect("the table has a lord") as SeatId;
        let mut others: Vec<_> = roster
            .iter()
            .filter(|candidate| candidate.id != hero_id)
            .collect();
        random.shuffle(&mut others);
        let seats = roles
            .into_iter()
            .enumerate()
            .map(|(index, role)| {
                let id = index as SeatId;
                let general = if id == human {
                    hero
                } else {
                    others.pop().expect("there are enough generals")
                };
                let max_hp = general.max_hp + u16::from(role == Role::Lord && count > 4);
                Seat {
                    id,
                    hp: i32::from(max_hp),
                    max_hp,
                    alive: true,
                    hero: Some(general),
                    role: Some(role),
                    hand: Vec::new(),
                    equipment: Vec::new(),
                    judges: Vec::new(),
                }
            })
            .collect();

        let raw_deck = classic_data::standard_deck();
        let mut faces = Vec::with_capacity(raw_deck.len());
        let mut deck: Vec<Card> = raw_deck
            .into_iter()
            .map(|physical| {
                faces.push(CardFace {
                    id: physical.id,
                    suit: physical.suit,
                    rank: physical.rank,
                });
                let kind = serde_json::from_value(serde_json::Value::String(physical.kind))
                    .expect("each bundled card kind has a protocol identifier");
                Card {
                    id: physical.id,
                    kind,
                }
            })
            .collect();
        random.shuffle(&mut deck);

        let mut game = Self::with_seats(seats, lord);
        game.classic = Some(Classic {
            human,
            lord,
            deck,
            faces,
            random,
            turn: None,
            sha_played: false,
            result: None,
            result_announced: false,
        });
        game.scheduler
            .start(Program::event(EventKind::Opening { lord }));
        game.run(None);
        Ok(game)
    }

    pub(super) fn match_result(&self) -> Option<MatchResult> {
        self.classic.as_ref().and_then(|state| state.result)
    }

    pub(super) fn play_choices(&self, seat: SeatId) -> Vec<Action> {
        let player = self.seat(seat).expect("the acting seat exists");
        let mut actions = vec![Action::EndPlay];
        for card in &player.hand {
            if let Some(uses) = self.card_uses(player, *card) {
                actions.extend(uses.into_iter().map(|planned| Action::PlayCard {
                    card: card.id,
                    targets: planned.targets,
                }));
            }
        }
        actions
    }

    fn card_uses(&self, player: &Seat, card: Card) -> Option<Vec<PlannedUse>> {
        let seat = player.id;
        let classic = self.classic.as_ref().expect("the table is classic");
        let uses = match card.kind {
            CardKind::Sha if !classic.sha_played => self
                .living_order_from(seat)
                .into_iter()
                .filter(|&target| self.distance(seat, target) == Ok(1))
                .map(|target| PlannedUse {
                    targets: vec![target],
                    program: Program::event(EventKind::UseSha { seat, target, card }),
                })
                .collect(),
            CardKind::Tao if player.hp < i32::from(player.max_hp) => vec![PlannedUse {
                targets: vec![seat],
                program: Program::event(EventKind::UseTao {
                    seat,
                    target: seat,
                    card,
                }),
            }],
            CardKind::Sha | CardKind::Shan | CardKind::Tao => Vec::new(),
            _ => return None,
        };
        Some(uses)
    }

    pub(super) fn plan_play(
        &self,
        seat: SeatId,
        card: CardId,
        targets: Vec<SeatId>,
    ) -> Result<Program, RuleError> {
        self.checked_play_seat(seat)?;
        let player = self.seat(seat)?;
        let card = player
            .hand
            .iter()
            .find(|held| held.id == card)
            .copied()
            .ok_or(RuleError::InvalidCard)?;
        self.card_uses(player, card)
            .ok_or(RuleError::UnsupportedCard)?
            .into_iter()
            .find(|planned| planned.targets == targets)
            .map(|planned| planned.program)
            .ok_or(RuleError::InvalidTarget)
    }

    /// Choose a single legal command for the next non-human seat.
    pub fn bot_command(&self) -> Option<Command> {
        let state = self.classic.as_ref()?;
        let decision = self.decision.as_ref()?;
        let seat = decision.seat();
        if seat == state.human {
            return None;
        }
        let player = self.seat(seat).ok()?;
        let action = match decision {
            Decision::Play(_) => self.bot_play(player),
            Decision::Cards(choice) => match choice.kind {
                ChoiceKind::Discard => {
                    let mut cards: Vec<_> = player
                        .hand
                        .iter()
                        .filter(|card| choice.cards.contains(&card.id))
                        .collect();
                    cards.sort_by_key(|card| match card.kind {
                        CardKind::Tao => 3,
                        CardKind::Shan => 2,
                        CardKind::Sha => 1,
                        _ => 0,
                    });
                    Action::Discard {
                        event: choice.event,
                        cards: cards
                            .into_iter()
                            .take(choice.min)
                            .map(|card| card.id)
                            .collect(),
                    }
                }
                ChoiceKind::Save => {
                    let save_lord = choice.target == Some(state.lord)
                        && (player.role == Some(Role::Loyalist)
                            || (player.role == Some(Role::Renegade)
                                && self.seats.iter().filter(|seat| seat.alive).count() > 2));
                    let card = if choice.target == Some(seat) || save_lord {
                        choice.cards.first().copied()
                    } else {
                        None
                    };
                    Action::Choose {
                        event: choice.event,
                        card,
                    }
                }
                ChoiceKind::Shan => Action::Choose {
                    event: choice.event,
                    card: choice.cards.first().copied(),
                },
            },
        };
        Some(Command {
            version: self.version,
            seat,
            action,
        })
    }

    fn bot_play(&self, player: &Seat) -> Action {
        let lord = self.classic.as_ref().expect("the table is classic").lord;
        self.play_choices(player.id)
            .into_iter()
            .max_by_key(|action| match action {
                Action::PlayCard { card, targets } => {
                    let held = player
                        .hand
                        .iter()
                        .find(|held| held.id == *card)
                        .expect("the option uses a held card");
                    if held.kind == CardKind::Tao {
                        (4, 0)
                    } else {
                        let target = self
                            .seat(targets[0])
                            .expect("the option names a living target");
                        let priority = if target.id != lord {
                            2
                        } else if player.role == Some(Role::Loyalist) {
                            0
                        } else {
                            3
                        };
                        (priority, -target.hp)
                    }
                }
                _ => (1, 0),
            })
            .expect("ending play is always available")
    }
}
