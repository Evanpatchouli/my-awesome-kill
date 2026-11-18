use super::PlannedUse;
use crate::scheduler::{EventRef, Outcome};
use crate::{
    Card, CardKind, CardTargetTiming, ChoiceBranch, ChoiceRequest, DrawReason, EventKind, Game,
    Program, Resolution, RuleError, Seat, SeatId, Zone,
};

// A trick is one scheduler event; its private state is carried across child choices.
#[derive(Clone)]
pub(crate) struct Trick {
    base_damage: u16,
    seat: SeatId,
    card: Option<Card>,
    kind: CardKind,
    costs: Vec<Card>,
    counterable: bool,
    initial_target: Option<SeatId>,
    targets: Vec<SeatId>,
    borrowed_victims: Vec<Option<SeatId>>,
    target_index: usize,
    stage: TrickStage,
    pool: Vec<Card>,
    judgment: bool,
}

#[derive(Clone)]
enum TrickStage {
    AfterUse,
    Start,
    AfterCosts,
    AfterUseCard2,
    TargetStart,
    TargetTimings { layer: usize, index: usize },
    BeginEffects,
    BorrowedRequest,
    BorrowedPrepare(usize),
    BorrowedVictim(usize),
    Counter,
    CounterWait,
    Effect,
    Response { actor: SeatId },
    Duel { actor: SeatId, remaining: u8 },
    Select,
    ApplyJudgment(Option<Card>),
}

impl Trick {
    #[cfg(test)]
    pub(crate) fn judgment_effect_name(&self) -> Option<&'static str> {
        match (&self.stage, self.kind) {
            (TrickStage::ApplyJudgment(_), CardKind::Lebu) => Some("lebu"),
            (TrickStage::ApplyJudgment(None), CardKind::Shandian) => Some("shandianCancel"),
            (TrickStage::ApplyJudgment(Some(_)), CardKind::Shandian) => Some("shandian"),
            _ => None,
        }
    }
    #[cfg(test)]
    pub(crate) fn judgment_effect_material(&self) -> Option<Card> {
        self.judgment_effect_name().and(self.card)
    }
    pub(crate) fn base_damage_mut(&mut self) -> Option<&mut u16> {
        (matches!(self.stage, TrickStage::AfterUseCard2)
            && matches!(
                self.kind,
                CardKind::Juedou | CardKind::Nanman | CardKind::Wanjian
            ))
        .then_some(&mut self.base_damage)
    }
    pub(crate) fn use_targets(&self) -> Option<&[SeatId]> {
        (!self.judgment).then_some(&self.targets)
    }

    pub(crate) fn editable_targets(&mut self) -> Option<&mut Vec<SeatId>> {
        matches!(self.stage, TrickStage::AfterUseCard2).then_some(&mut self.targets)
    }

    pub(crate) fn kind(&self) -> EventKind {
        if matches!(self.stage, TrickStage::ApplyJudgment(_)) {
            return EventKind::TrickEffect {
                seat: self.seat,
                target: self.seat,
                kind: self.kind,
            };
        }
        match self.card {
            Some(card) => EventKind::UseTrick {
                seat: self.seat,
                card,
                target: self.initial_target,
            },
            None => EventKind::UseVirtual {
                seat: self.seat,
                kind: self.kind,
            },
        }
    }
}

impl Game {
    pub(crate) fn judgment_effect_program(
        &self,
        seat: SeatId,
        card: Card,
        kind: CardKind,
        judged: Option<Card>,
    ) -> Program {
        Program::Trick(Trick {
            base_damage: 1,
            seat,
            card: Some(card),
            kind,
            costs: vec![],
            counterable: true,
            initial_target: Some(seat),
            targets: vec![seat],
            borrowed_victims: vec![],
            target_index: 0,
            stage: TrickStage::ApplyJudgment(judged),
            pool: vec![],
            judgment: true,
        })
    }

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
            Tiesuo => {
                let mut choices = Vec::new();
                for (i, &first) in living.iter().enumerate() {
                    choices.push(vec![first]);
                    choices.extend(living[i + 1..].iter().map(|&second| vec![first, second]));
                }
                choices
            }
            Wugu | Taoyuan => vec![living],
            Nanman | Wanjian => vec![others],
            Wuzhong | Shandian => vec![vec![seat]],
            Juedou => others
                .into_iter()
                .filter(|target| self.hero_allows_target(*target, Juedou))
                .map(|target| vec![target])
                .collect(),
            Lebu => others
                .into_iter()
                .filter(|target| {
                    self.hero_allows_target(*target, Lebu)
                        && !self
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
                    self.hero_allows_target(*target, card.kind)
                        && (card.kind != Shunshou
                            || self.hero_trick_unlimited_range(seat)
                            || self.distance(seat, *target) == Ok(1))
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
                                && self.hero_allows_target(*target, Sha)
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
                        base_damage: 1,
                        seat,
                        card: Some(card),
                        kind: card.kind,
                        costs: vec![card],
                        counterable: true,
                        initial_target: targets.first().copied(),
                        borrowed_victims: if card.kind == Jiedao {
                            vec![Some(targets[1])]
                        } else {
                            vec![]
                        },
                        targets: if card.kind == Jiedao {
                            vec![targets[0]]
                        } else {
                            targets
                        },
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
            base_damage: 1,
            seat,
            card: None,
            kind: CardKind::Juedou,
            costs: vec![],
            counterable: false,
            initial_target: Some(target),
            targets: vec![target],
            borrowed_victims: vec![],
            target_index: 0,
            stage: TrickStage::Start,
            pool: vec![],
            judgment: false,
        })
    }
}

impl Game {
    fn pass_lightning(&mut self, target: SeatId, material: Card) {
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
                    .any(|card| self.judgment_kind(*card) == CardKind::Shandian)
            });
        if let Some(next) = next.filter(|_| self.resolving.contains(&material)) {
            let card = self.take_resolving(material.id);
            self.put_in_judgment(next, card, CardKind::Shandian);
        }
    }

    fn advance_trick(&mut self, mut flow: Trick) {
        flow.target_index += 1;
        flow.stage = TrickStage::Counter;
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
            if matches!(flow.kind, Lebu | Shandian) && !flow.judgment {
                for card in &flow.costs {
                    if self.resolving.contains(card) {
                        self.scheduler
                            .after(Program::event(EventKind::Discard { card: *card }));
                    }
                }
            }
            for card in flow.pool {
                self.scheduler
                    .next(Program::event(EventKind::Discard { card }));
            }
            self.finish();
            return;
        }
        match flow.stage.clone() {
            AfterUse => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    self.scheduler.end(Outcome::Cancelled);
                } else {
                    self.finish();
                }
            }
            Start => {
                self.queue_use_costs(flow.seat, &flow.costs);
                if !matches!(flow.kind, Lebu | Shandian) {
                    for card in &flow.costs {
                        self.scheduler
                            .after(Program::event(EventKind::Discard { card: *card }));
                    }
                }
                flow.stage = AfterCosts;
                self.scheduler.replace_current(Program::Trick(flow));
            }
            AfterCosts => {
                flow.stage = AfterUseCard2;
                self.scheduler.replace_current(Program::Trick(flow.clone()));
                self.queue_use_card2(flow.seat, flow.kind, &flow.costs);
            }
            AfterUseCard2 => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    if matches!(flow.kind, Lebu | Shandian) {
                        for card in &flow.costs {
                            self.scheduler
                                .after(Program::event(EventKind::Discard { card: *card }));
                        }
                    }
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                if !matches!(flow.kind, Lebu | Shandian) {
                    self.queue_hero_moment(super::skills::HeroMoment::UseTrick {
                        seat: flow.seat,
                        kind: flow.kind,
                        cards: flow.costs.clone(),
                    });
                }
                flow.stage = TargetStart;
                self.scheduler.replace_current(Program::Trick(flow));
            }
            TargetStart => {
                if matches!(flow.kind, Lebu | Shandian)
                    || !CardTargetTiming::ALL
                        .iter()
                        .any(|timing| self.wants_timing(timing.name()))
                {
                    flow.stage = BeginEffects;
                } else {
                    self.sort_card_targets(&mut flow.targets);
                    flow.stage = TargetTimings { layer: 0, index: 0 };
                }
                self.scheduler.replace_current(Program::Trick(flow));
            }
            TargetTimings {
                mut layer,
                mut index,
            } => {
                while layer < CardTargetTiming::ALL.len() {
                    if index == flow.targets.len() {
                        layer += 1;
                        index = 0;
                        continue;
                    }
                    let timing = CardTargetTiming::ALL[layer];
                    let target = flow.targets[index];
                    index += 1;
                    if self.queue_card_target_timing(
                        timing,
                        flow.seat,
                        target,
                        flow.kind,
                        &flow.costs,
                    ) {
                        flow.stage = TargetTimings { layer, index };
                        self.scheduler.replace_current(Program::Trick(flow));
                        return;
                    }
                }
                flow.stage = BeginEffects;
                self.scheduler.replace_current(Program::Trick(flow));
            }
            BeginEffects => {
                self.sort_card_targets(&mut flow.targets);
                if matches!(flow.kind, Lebu | Shandian) {
                    let Some(&target) = flow.targets.first() else {
                        for card in &flow.costs {
                            self.scheduler
                                .after(Program::event(EventKind::Discard { card: *card }));
                        }
                        self.finish();
                        return;
                    };
                    let material = flow.costs[0];
                    let card = self.take_resolving(material.id);
                    self.put_in_judgment(target, card, flow.kind);
                    self.finish();
                    return;
                }
                if flow.kind == Wugu {
                    let Some(cards) = self.reveal_cards(flow.targets.len()) else {
                        self.scheduler.end(Outcome::Cancelled);
                        return;
                    };
                    flow.pool = cards;
                }
                flow.stage =
                    if flow.kind == Jiedao && flow.borrowed_victims.len() < flow.targets.len() {
                        BorrowedPrepare(0)
                    } else {
                        Counter
                    };
                self.scheduler.replace_current(Program::Trick(flow));
            }
            Counter => {
                if flow.target_index >= flow.targets.len() {
                    for card in flow.pool.drain(..) {
                        self.scheduler
                            .next(Program::event(EventKind::Discard { card }));
                    }
                    let last_effect = flow.target_index.saturating_sub(1) as u16;
                    if !flow.judgment
                        && self.queue_timing(
                            "useCardAfter",
                            Some(flow.seat),
                            None,
                            None,
                            last_effect,
                            &flow.costs,
                        )
                    {
                        flow.stage = AfterUse;
                        self.scheduler.replace_current(Program::Trick(flow));
                    } else {
                        self.finish();
                    }
                    return;
                }
                let target = flow.targets[flow.target_index];
                if !self.seat(target).is_ok_and(|seat| seat.alive) {
                    self.advance_trick(flow);
                    return;
                }
                if flow.counterable {
                    let window = self.counter_window(crate::CounterContext {
                        card_kind: flow.kind,
                        source: Some(flow.seat),
                        target,
                        negated: false,
                    });
                    flow.stage = CounterWait;
                    self.scheduler.replace_current(Program::Trick(flow));
                    self.scheduler.next(window);
                } else {
                    flow.stage = Effect;
                    self.scheduler.replace_current(Program::Trick(flow));
                }
            }
            CounterWait => {
                let target = flow.targets[flow.target_index];
                if matches!(child, Some(Outcome::Finished(Resolution::Countered(false))))
                    && self.is_alive(target)
                {
                    flow.stage = Effect;
                    self.scheduler.replace_current(Program::Trick(flow));
                } else {
                    self.advance_trick(flow);
                }
            }
            Effect => {
                let target = flow.targets[flow.target_index];
                match flow.kind {
                    Tiesuo => {
                        self.scheduler
                            .next(Program::Elemental(super::elemental::Flow::Tiesuo {
                                seat: flow.seat,
                                target,
                            }));
                        self.advance_trick(flow);
                    }
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
                        flow.stage = Duel {
                            actor: target,
                            remaining: self.hero_response_count(flow.seat, Sha),
                        };
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
                        let victim = flow
                            .borrowed_victims
                            .get(flow.target_index)
                            .copied()
                            .flatten();
                        if !victim.is_some_and(|victim| self.seat(victim).is_ok_and(|p| p.alive)) {
                            if let Some(weapon) = self.weapon(target) {
                                let card = self.take_card(target, Zone::Equipment, weapon.id);
                                self.put_in_hand(flow.seat, card);
                                self.queue_hero_losses(target, vec![(Zone::Equipment, card)]);
                            }
                            self.advance_trick(flow);
                            return;
                        }
                        let victim = victim.expect("checked above");
                        let mut branches = self.sha_use_branches(target, &[victim]);
                        if self.card_enabled(target, Sha)
                            && self.accepts_sha_target(target, victim)
                            && self.hero_lord_request_available(target, Sha)
                        {
                            branches.push(ChoiceBranch {
                                id: 1000,
                                label: "激将".into(),
                                cards: vec![],
                                min_cards: 0,
                                max_cards: 0,
                                targets: vec![],
                                min_targets: 0,
                                max_targets: 0,
                            });
                        }
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
                    self.scheduler.next(self.damage_program(
                        Some(flow.seat),
                        actor,
                        flow.base_damage,
                        Some(flow.kind),
                        flow.costs.clone(),
                    ));
                }
                self.advance_trick(flow);
            }
            Duel { actor, remaining } => {
                if matches!(child, Some(Outcome::Finished(Resolution::Responded))) {
                    if remaining > 1 {
                        flow.stage = Duel {
                            actor,
                            remaining: remaining - 1,
                        };
                        self.scheduler.replace_current(Program::Trick(flow));
                        self.scheduler.next(Program::respond(actor, Sha));
                        return;
                    }
                    let next = if actor == flow.seat {
                        flow.targets[flow.target_index]
                    } else {
                        flow.seat
                    };
                    flow.stage = Duel {
                        actor: next,
                        remaining: self.hero_response_count(actor, Sha),
                    };
                    self.scheduler.replace_current(Program::Trick(flow));
                    self.scheduler.next(Program::respond(next, Sha));
                } else {
                    let source = if actor == flow.seat {
                        flow.targets[flow.target_index]
                    } else {
                        flow.seat
                    };
                    self.scheduler.next(self.damage_program(
                        Some(source),
                        actor,
                        flow.base_damage,
                        Some(flow.kind),
                        flow.costs.clone(),
                    ));
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
                                self.queue_hero_losses(target, vec![(zone, card)]);
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
                        let victim = flow.borrowed_victims[flow.target_index]
                            .expect("borrowed holder has a victim");
                        if selection.branch == 1000 {
                            let request = self
                                .hero_lord_request(target, Sha, vec![victim])
                                .expect("offered lord request");
                            flow.stage = BorrowedRequest;
                            self.scheduler.replace_current(Program::Trick(flow));
                            self.scheduler.next(request);
                            return;
                        } else if selection.branch == 0 {
                            if let Some(weapon) = self.weapon(target) {
                                let card = self.take_card(target, Zone::Equipment, weapon.id);
                                self.put_in_hand(flow.seat, card);
                                self.queue_hero_losses(target, vec![(Zone::Equipment, card)]);
                            }
                        } else {
                            let cards = self.hero_cards_by_id(target, &selection.cards);
                            self.scheduler
                                .next(self.attack_program(target, cards, vec![victim]));
                        }
                    }
                    _ => {}
                }
                self.advance_trick(flow);
            }
            BorrowedRequest => {
                if matches!(child, Some(Outcome::Finished(Resolution::Passed))) {
                    let holder = flow.targets[flow.target_index];
                    let victim = flow.borrowed_victims[flow.target_index]
                        .expect("borrowed holder has a victim");
                    let branches = self.sha_use_branches(holder, &[victim]);
                    flow.stage = Select;
                    self.scheduler.replace_current(Program::Trick(flow));
                    self.scheduler.next(Program::Choose(ChoiceRequest {
                        seat: holder,
                        target: None,
                        branches,
                    }));
                } else {
                    self.advance_trick(flow);
                }
            }
            BorrowedPrepare(index) => {
                if index == flow.targets.len() {
                    flow.stage = Counter;
                } else {
                    let holder = flow.targets[index];
                    if Some(holder) == flow.initial_target {
                        flow.borrowed_victims.push(flow.borrowed_victims[0]);
                        flow.stage = BorrowedPrepare(index + 1);
                    } else {
                        let victims = self.jiedao_victims(holder);
                        if victims.is_empty() {
                            flow.borrowed_victims.push(None);
                            flow.stage = BorrowedPrepare(index + 1);
                        } else {
                            let seat = flow.seat;
                            flow.stage = BorrowedVictim(index);
                            self.scheduler.replace_current(Program::Trick(flow));
                            self.scheduler.next(Program::Choose(ChoiceRequest {
                                seat,
                                target: Some(holder),
                                branches: vec![ChoiceBranch {
                                    id: 0,
                                    label: "选择被杀目标".into(),
                                    cards: vec![],
                                    min_cards: 0,
                                    max_cards: 0,
                                    targets: victims,
                                    min_targets: 1,
                                    max_targets: 1,
                                }],
                            }));
                            return;
                        }
                    }
                }
                self.scheduler.replace_current(Program::Trick(flow));
            }
            BorrowedVictim(index) => {
                let victim = match child {
                    Some(Outcome::Finished(Resolution::Selected(selection))) => {
                        Some(selection.targets[0])
                    }
                    _ => None,
                };
                flow.borrowed_victims.push(victim);
                flow.stage = BorrowedPrepare(index + 1);
                self.scheduler.replace_current(Program::Trick(flow));
            }
            ApplyJudgment(judged) => {
                let material = flow.card.expect("delayed material");
                if flow.kind == Shandian {
                    let hit = judged
                        .and_then(|card| self.face(card.id))
                        .is_some_and(|face| {
                            face.suit == crate::classic_data::Suit::Spade
                                && (2..=9).contains(&face.rank)
                        });
                    if hit {
                        self.scheduler.next(self.damage_program(
                            None,
                            flow.seat,
                            3,
                            Some(Shandian),
                            vec![material],
                        ));
                    } else {
                        self.pass_lightning(flow.seat, material);
                    }
                } else if flow.kind == Lebu {
                    if judged
                        .and_then(|card| self.face(card.id))
                        .is_some_and(|face| face.suit != crate::classic_data::Suit::Heart)
                    {
                        self.skip_play_phase(flow.seat);
                    }
                }
                self.finish();
            }
        }
    }
}
