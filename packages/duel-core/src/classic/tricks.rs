use super::PlannedUse;
use crate::scheduler::{EventRef, Outcome};
use crate::{
    Card, CardKind, ChoiceBranch, ChoiceRequest, DrawReason, EventKind, Game, Program, Resolution,
    RuleError, Seat, SeatId, Zone,
};

// A trick is one scheduler event; its private state is carried across child choices.
#[derive(Clone)]
pub(crate) struct Trick {
    seat: SeatId,
    card: Option<Card>,
    kind: CardKind,
    costs: Vec<Card>,
    counterable: bool,
    targets: Vec<SeatId>,
    target_index: usize,
    stage: TrickStage,
    pool: Vec<Card>,
    judgment: bool,
}

#[derive(Clone)]
enum TrickStage {
    Start,
    AfterCosts,
    Counter {
        index: usize,
        passed: usize,
        negated: bool,
    },
    CounterWait {
        index: usize,
        passed: usize,
        negated: bool,
    },
    Effect,
    Response {
        actor: SeatId,
    },
    Duel {
        actor: SeatId,
    },
    Select,
    Judgment,
    JudgmentResult,
}

impl Trick {
    pub(crate) fn kind(&self) -> EventKind {
        match self.card {
            Some(card) => EventKind::UseTrick {
                seat: self.seat,
                card,
                target: self.targets.first().copied(),
            },
            None => EventKind::UseVirtual {
                seat: self.seat,
                kind: self.kind,
            },
        }
    }
}

impl Game {
    pub(crate) fn trick_uses(&self, player: &Seat, card: Card) -> Option<Vec<PlannedUse>> {
        use CardKind::*;
        let seat = player.id;
        let living = self.living_order_from(seat);
        let others: Vec<_> = living
            .iter()
            .copied()
            .filter(|target| *target != seat)
            .collect();
        let target_lists: Vec<Vec<SeatId>> = match card.kind {
            Wugu | Taoyuan => vec![living],
            Nanman | Wanjian => vec![others],
            Wuzhong | Shandian => vec![vec![seat]],
            Juedou => others.into_iter().map(|target| vec![target]).collect(),
            Lebu => others
                .into_iter()
                .filter(|target| {
                    !self
                        .seat(*target)
                        .expect("living target")
                        .judges
                        .iter()
                        .any(|c| self.judgment_kind(*c) == Lebu)
                })
                .map(|target| vec![target])
                .collect(),
            Shunshou | Guohe => others
                .into_iter()
                .filter(|target| {
                    let p = self.seat(*target).expect("living target");
                    (card.kind != Shunshou || self.distance(seat, *target) == Ok(1))
                        && (!p.hand.is_empty() || !p.equipment.is_empty() || !p.judges.is_empty())
                })
                .map(|target| vec![target])
                .collect(),
            Jiedao => others
                .into_iter()
                .flat_map(|holder| {
                    self.living_order_from(holder)
                        .into_iter()
                        .filter(move |target| {
                            *target != holder
                                && self.weapon(holder).is_some()
                                && self.in_sha_range(holder, *target)
                        })
                        .map(move |target| vec![holder, target])
                })
                .collect(),
            Wuxie => Vec::new(),
            _ => return None,
        };
        Some(
            target_lists
                .into_iter()
                .filter(|targets| !targets.is_empty())
                .filter(|_targets| {
                    card.kind != Shandian
                        || !player
                            .judges
                            .iter()
                            .any(|c| self.judgment_kind(*c) == Shandian)
                })
                .map(|targets| PlannedUse {
                    targets: targets.clone(),
                    program: Program::Trick(Trick {
                        seat,
                        card: Some(card),
                        kind: card.kind,
                        costs: vec![card],
                        counterable: true,
                        targets,
                        target_index: 0,
                        stage: TrickStage::Start,
                        pool: Vec::new(),
                        judgment: false,
                    }),
                })
                .collect(),
        )
    }
}

impl Game {
    pub(crate) fn virtual_trick_program(
        &self,
        seat: SeatId,
        kind: CardKind,
        costs: Vec<Card>,
        targets: Vec<SeatId>,
    ) -> Result<Program, RuleError> {
        if !matches!(kind, CardKind::Guohe | CardKind::Lebu) || costs.len() != 1 {
            return Err(RuleError::InvalidSelection);
        }
        let player = self.seat(seat)?;
        let material = costs[0];
        if !player.hand.contains(&material) && !player.equipment.contains(&material) {
            return Err(RuleError::InvalidCard);
        }
        // This descriptor is used only to calculate legal targets, never stored or emitted.
        let descriptor = Card {
            id: material.id,
            kind,
        };
        let planned = self
            .trick_uses(player, descriptor)
            .ok_or(RuleError::UnsupportedCard)?
            .into_iter()
            .find(|item| item.targets == targets)
            .ok_or(RuleError::InvalidTarget)?;
        let Program::Trick(mut flow) = planned.program else {
            unreachable!()
        };
        flow.card = None;
        flow.costs = costs;
        Ok(Program::Trick(flow))
    }

    pub(crate) fn virtual_duel_without_counter(&self, seat: SeatId, target: SeatId) -> Program {
        Program::Trick(Trick {
            seat,
            card: None,
            kind: CardKind::Juedou,
            costs: vec![],
            counterable: false,
            targets: vec![target],
            target_index: 0,
            stage: TrickStage::Start,
            pool: vec![],
            judgment: false,
        })
    }
}

impl Game {
    pub(crate) fn trick_judgment(&self, seat: SeatId, card: Card) -> Program {
        Program::Trick(Trick {
            seat,
            card: Some(card),
            kind: self.judgment_kind(card),
            costs: vec![],
            counterable: true,
            targets: vec![seat],
            target_index: 0,
            stage: TrickStage::Counter {
                index: 0,
                passed: 0,
                negated: false,
            },
            pool: Vec::new(),
            judgment: true,
        })
    }

    fn advance_trick(&mut self, mut flow: Trick) {
        flow.target_index = if flow.kind == CardKind::Jiedao {
            flow.targets.len()
        } else {
            flow.target_index + 1
        };
        flow.stage = TrickStage::Counter {
            index: 0,
            passed: 0,
            negated: false,
        };
        self.scheduler.replace_current(Program::Trick(flow));
    }

    pub(crate) fn resolve_trick(
        &mut self,
        _event: EventRef,
        mut flow: Trick,
        child: Option<Outcome<Resolution>>,
    ) {
        use CardKind::*;
        use TrickStage::*;
        if self.match_result().is_some() {
            for card in flow.pool {
                self.scheduler
                    .next(Program::event(EventKind::Discard { card }));
            }
            self.finish();
            return;
        }
        match flow.stage.clone() {
            Start => {
                if let Some(card) = flow.card {
                    self.move_to_resolving(flow.seat, card.id);
                } else {
                    for card in &flow.costs {
                        self.next(EventKind::UseCost {
                            seat: flow.seat,
                            card: *card,
                        });
                    }
                }
                flow.stage = AfterCosts;
                self.scheduler.replace_current(Program::Trick(flow));
            }
            AfterCosts => {
                if matches!(flow.kind, Lebu | Shandian) {
                    let target = flow.targets[0];
                    let material = flow.costs[0];
                    let card = self.take_resolving(material.id);
                    self.put_in_judgment(target, card, flow.kind);
                    self.finish();
                    return;
                }
                for card in &flow.costs {
                    self.scheduler
                        .after(Program::event(EventKind::Discard { card: *card }));
                }
                if flow.kind == Wugu {
                    let Some(cards) = self.reveal_cards(flow.targets.len()) else {
                        self.scheduler.end(Outcome::Cancelled);
                        return;
                    };
                    flow.pool = cards;
                }
                flow.stage = if flow.counterable {
                    Counter {
                        index: 0,
                        passed: 0,
                        negated: false,
                    }
                } else {
                    Effect
                };
                self.scheduler.replace_current(Program::Trick(flow));
            }
            Counter {
                index,
                passed,
                negated,
            } => {
                if flow.target_index >= flow.targets.len() {
                    for card in flow.pool {
                        self.scheduler
                            .next(Program::event(EventKind::Discard { card }));
                    }
                    self.finish();
                    return;
                }
                let target = flow.targets[flow.target_index];
                if !self.seat(target).is_ok_and(|seat| seat.alive) {
                    self.advance_trick(flow);
                    return;
                }
                let order = self.living_order_from(self.active_seat);
                if passed >= order.len() {
                    if negated {
                        if flow.judgment {
                            self.scheduler.next(Program::DiscardFrom {
                                seat: target,
                                zone: Zone::Judgment,
                                card: flow.card.expect("judgment has a physical card"),
                            });
                            self.finish();
                        } else {
                            self.advance_trick(flow);
                        }
                    } else {
                        flow.stage = if flow.judgment { Judgment } else { Effect };
                        self.scheduler.replace_current(Program::Trick(flow));
                    }
                    return;
                }
                let actor = order[index % order.len()];
                flow.stage = CounterWait {
                    index,
                    passed,
                    negated,
                };
                self.scheduler.replace_current(Program::Trick(flow));
                self.scheduler.next(Program::respond(actor, Wuxie));
            }
            CounterWait {
                index,
                passed,
                negated,
            } => {
                let responded = matches!(child, Some(Outcome::Finished(Resolution::Responded)));
                flow.stage = Counter {
                    index: index + 1,
                    passed: if responded { 0 } else { passed + 1 },
                    negated: if responded { !negated } else { negated },
                };
                self.scheduler.replace_current(Program::Trick(flow));
            }
            Effect => {
                let target = flow.targets[flow.target_index];
                match flow.kind {
                    Wuzhong => {
                        self.queue_draw(target, 2, DrawReason::CardEffect);
                        self.advance_trick(flow);
                    }
                    Taoyuan => {
                        self.next(EventKind::Recover {
                            seat: flow.seat,
                            target,
                            amount: 1,
                        });
                        self.advance_trick(flow);
                    }
                    Nanman | Wanjian => {
                        let kind = if flow.kind == Nanman { Sha } else { Shan };
                        flow.stage = Response { actor: target };
                        self.scheduler.replace_current(Program::Trick(flow));
                        self.scheduler.next(Program::respond(target, kind));
                    }
                    Juedou => {
                        flow.stage = Duel { actor: target };
                        self.scheduler.replace_current(Program::Trick(flow));
                        self.scheduler.next(Program::respond(target, Sha));
                    }
                    Wugu => {
                        if flow.pool.is_empty() {
                            self.advance_trick(flow);
                            return;
                        }
                        let branch = ChoiceBranch {
                            id: 0,
                            label: "选择一张公开牌".into(),
                            cards: flow.pool.iter().map(|c| c.id).collect(),
                            min_cards: 1,
                            max_cards: 1,
                            targets: vec![],
                            min_targets: 0,
                            max_targets: 0,
                        };
                        flow.stage = Select;
                        self.scheduler.replace_current(Program::Trick(flow));
                        self.scheduler.next(Program::Choose(ChoiceRequest {
                            seat: target,
                            target: None,
                            branches: vec![branch],
                        }));
                    }
                    Shunshou | Guohe => {
                        let p = self.seat(target).expect("target exists");
                        let mut branches = Vec::new();
                        if !p.hand.is_empty() {
                            branches.push(ChoiceBranch {
                                id: 0,
                                label: "手牌区（随机）".into(),
                                cards: vec![],
                                min_cards: 0,
                                max_cards: 0,
                                targets: vec![],
                                min_targets: 0,
                                max_targets: 0,
                            });
                        }
                        let public: Vec<_> =
                            p.equipment.iter().chain(&p.judges).map(|c| c.id).collect();
                        if !public.is_empty() {
                            branches.push(ChoiceBranch {
                                id: 1,
                                label: "装备或判定牌".into(),
                                cards: public,
                                min_cards: 1,
                                max_cards: 1,
                                targets: vec![],
                                min_targets: 0,
                                max_targets: 0,
                            });
                        }
                        if branches.is_empty() {
                            self.advance_trick(flow);
                            return;
                        }
                        let seat = flow.seat;
                        flow.stage = Select;
                        self.scheduler.replace_current(Program::Trick(flow));
                        self.scheduler.next(Program::Choose(ChoiceRequest {
                            seat,
                            target: Some(target),
                            branches,
                        }));
                    }
                    Jiedao => {
                        let victim = flow.targets[1];
                        let branches = self.sha_use_branches(target, &[victim]);
                        flow.stage = Select;
                        self.scheduler.replace_current(Program::Trick(flow));
                        self.scheduler.next(Program::Choose(ChoiceRequest {
                            seat: target,
                            target: Some(victim),
                            branches,
                        }));
                    }
                    _ => self.advance_trick(flow),
                }
            }
            Response { actor } => {
                if !matches!(child, Some(Outcome::Finished(Resolution::Responded))) {
                    self.next(EventKind::Damage {
                        source: Some(flow.seat),
                        target: actor,
                        amount: 1,
                    });
                }
                self.advance_trick(flow);
            }
            Duel { actor } => {
                if matches!(child, Some(Outcome::Finished(Resolution::Responded))) {
                    let next = if actor == flow.seat {
                        flow.targets[flow.target_index]
                    } else {
                        flow.seat
                    };
                    flow.stage = Duel { actor: next };
                    self.scheduler.replace_current(Program::Trick(flow));
                    self.scheduler.next(Program::respond(next, Sha));
                } else {
                    let source = if actor == flow.seat {
                        flow.targets[flow.target_index]
                    } else {
                        flow.seat
                    };
                    self.next(EventKind::Damage {
                        source: Some(source),
                        target: actor,
                        amount: 1,
                    });
                    self.advance_trick(flow);
                }
            }
            Select => {
                let Some(Outcome::Finished(Resolution::Selected(selection))) = child else {
                    self.advance_trick(flow);
                    return;
                };
                let target = flow.targets[flow.target_index];
                match flow.kind {
                    Wugu => {
                        let id = selection.cards[0];
                        flow.pool.retain(|c| c.id != id);
                        let card = self.take_resolving(id);
                        self.put_in_hand(target, card);
                    }
                    Shunshou | Guohe => {
                        let found = if selection.branch == 0 {
                            self.random_hand_card(target).map(|card| (Zone::Hand, card))
                        } else {
                            let id = selection.cards[0];
                            let p = self.seat(target).expect("target exists");
                            p.equipment
                                .iter()
                                .find(|c| c.id == id)
                                .copied()
                                .map(|c| (Zone::Equipment, c))
                                .or_else(|| {
                                    p.judges
                                        .iter()
                                        .find(|c| c.id == id)
                                        .copied()
                                        .map(|c| (Zone::Judgment, c))
                                })
                        };
                        if let Some((zone, card)) = found {
                            if flow.kind == Shunshou {
                                let card = self.take_card(target, zone, card.id);
                                self.put_in_hand(flow.seat, card);
                            } else {
                                self.scheduler.next(Program::DiscardFrom {
                                    seat: target,
                                    zone,
                                    card,
                                });
                            }
                        }
                    }
                    Jiedao => {
                        if selection.branch == 0 {
                            if let Some(weapon) = self.weapon(target) {
                                let card = self.take_card(target, Zone::Equipment, weapon.id);
                                self.put_in_hand(flow.seat, card);
                            }
                        } else {
                            let cards = selection
                                .cards
                                .iter()
                                .filter_map(|id| {
                                    self.seat(target)
                                        .ok()?
                                        .hand
                                        .iter()
                                        .find(|c| c.id == *id)
                                        .copied()
                                })
                                .collect();
                            self.scheduler.next(self.attack_program(
                                target,
                                cards,
                                vec![flow.targets[1]],
                            ));
                        }
                    }
                    _ => {}
                }
                self.advance_trick(flow);
            }
            Judgment => {
                let target = flow.targets[0];
                flow.stage = JudgmentResult;
                self.scheduler.replace_current(Program::Trick(flow));
                self.scheduler.next(Program::Judge { seat: target });
            }
            JudgmentResult => {
                let target = flow.targets[0];
                let Some(Outcome::Finished(Resolution::Judged(card))) = child else {
                    self.finish();
                    return;
                };
                let face = self.face(card.id).expect("judgment card has a face");
                let hit =
                    face.suit == crate::classic_data::Suit::Spade && (2..=9).contains(&face.rank);
                if flow.kind == Lebu {
                    if face.suit != crate::classic_data::Suit::Heart {
                        self.skip_play_phase(target);
                    }
                    self.scheduler.next(Program::DiscardFrom {
                        seat: target,
                        zone: Zone::Judgment,
                        card: flow.card.expect("judgment has a physical card"),
                    });
                } else if hit {
                    self.scheduler.next(Program::DiscardFrom {
                        seat: target,
                        zone: Zone::Judgment,
                        card: flow.card.expect("judgment has a physical card"),
                    });
                    self.next(EventKind::Damage {
                        source: None,
                        target,
                        amount: 3,
                    });
                } else {
                    let next = self
                        .living_order_from(target)
                        .into_iter()
                        .skip(1)
                        .find(|id| {
                            !self
                                .seat(*id)
                                .expect("living seat")
                                .judges
                                .iter()
                                .any(|card| self.judgment_kind(*card) == Shandian)
                        });
                    if let Some(next) = next {
                        let card = self.take_card(
                            target,
                            Zone::Judgment,
                            flow.card.expect("judgment has a physical card").id,
                        );
                        self.put_in_judgment(next, card, flow.kind);
                    } else {
                        self.scheduler.next(Program::DiscardFrom {
                            seat: target,
                            zone: Zone::Judgment,
                            card: flow.card.expect("judgment has a physical card"),
                        });
                    }
                }
                self.finish();
            }
        }
    }
}
