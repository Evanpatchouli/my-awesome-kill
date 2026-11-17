use super::{branch, option, selection, HeroAdjustment, HeroFlow, HeroMoment, SkillId};
use crate::classic_data::Suit;
use crate::scheduler::Outcome;
use crate::{CardId, CardKind, DrawReason, EventKind, Game, Program, Resolution, SeatId, Zone};

#[derive(Clone)]
pub(crate) struct Queue {
    pub(super) moment: HeroMoment,
    pending: Option<Vec<(SeatId, SkillId)>>,
    adjustment: HeroAdjustment,
}

#[derive(Clone)]
pub(crate) struct Trigger {
    owner: SeatId,
    skill: SkillId,
    moment: HeroMoment,
    stage: Stage,
}

#[derive(Clone)]
enum Stage {
    Start,
    Option,
    Judge,
    Discard,
    GuanxingTop,
    GuanxingBottom { top: Vec<CardId> },
    Distribute,
    Repeat,
}

impl Trigger {
    pub(super) fn kind(&self) -> EventKind {
        EventKind::Skill {
            seat: self.owner,
            skill: self.skill,
        }
    }
}

impl Game {
    pub(crate) fn has_hero_triggers(&self, moment: &HeroMoment) -> bool {
        !self.register_hero_triggers(moment).is_empty()
    }

    pub(crate) fn hero_triggers(&self, moment: HeroMoment) -> Program {
        let adjustment = match moment {
            HeroMoment::Draw { amount, .. } => HeroAdjustment::Draw(amount),
            HeroMoment::JudgeBefore { card, .. } => HeroAdjustment::Judgment(card),
            _ => HeroAdjustment::None,
        };
        Program::Hero(HeroFlow::Triggers(Queue {
            moment,
            pending: None,
            adjustment,
        }))
    }

    pub(super) fn resolve_hero_triggers(
        &mut self,
        mut queue: Queue,
        child: Option<Outcome<Resolution>>,
    ) {
        if let Some(Outcome::Finished(Resolution::Hero(adjustment))) = child {
            match adjustment {
                HeroAdjustment::None => {}
                HeroAdjustment::Draw(amount) => {
                    let HeroMoment::Draw {
                        amount: current, ..
                    } = &mut queue.moment
                    else {
                        unreachable!()
                    };
                    *current = amount;
                    queue.adjustment = adjustment;
                }
                HeroAdjustment::Judgment(card) => {
                    let HeroMoment::JudgeBefore { card: current, .. } = &mut queue.moment else {
                        unreachable!()
                    };
                    *current = card;
                    queue.adjustment = adjustment;
                }
                HeroAdjustment::Skip | HeroAdjustment::Redirect(_) | HeroAdjustment::DirectHit => {
                    self.finish_hero(adjustment);
                    return;
                }
            }
        }
        if self.match_result().is_some() {
            self.finish_hero(queue.adjustment);
            return;
        }
        let pending = queue
            .pending
            .get_or_insert_with(|| self.register_hero_triggers(&queue.moment));
        while let Some((owner, skill)) = pending.pop() {
            if !self.seat(owner).is_ok_and(|seat| seat.alive) {
                continue;
            }
            let moment = queue.moment.clone();
            self.scheduler
                .replace_current(Program::Hero(HeroFlow::Triggers(queue)));
            self.scheduler
                .next(Program::Hero(HeroFlow::Trigger(Trigger {
                    owner,
                    skill,
                    moment,
                    stage: Stage::Start,
                })));
            return;
        }
        self.finish_hero(queue.adjustment);
    }

    fn register_hero_triggers(&self, moment: &HeroMoment) -> Vec<(SeatId, SkillId)> {
        use HeroMoment::*;
        use SkillId::*;
        let mut calls = Vec::new();
        let mut register = |seat, skill: SkillId, times| {
            if self.has_skill(seat, skill.as_str()) {
                calls.extend(std::iter::repeat_n((seat, skill), times));
            }
        };
        match *moment {
            Preparation { seat } => {
                register(seat, Guanxing, 1);
                register(seat, Luoshen, 1);
            }
            Draw { seat, .. } => {
                register(seat, Tuxi, 1);
                register(seat, Luoyi, 1);
                register(seat, Yingzi, 1);
            }
            Discard { seat } => register(seat, Keji, 1),
            End { seat } => register(seat, Biyue, 1),
            UseTrick { seat, kind } if !matches!(kind, CardKind::Lebu | CardKind::Shandian) => {
                register(seat, Jizhi, 1)
            }
            DamageAfter { target, amount, .. } => {
                register(target, Jianxiong, 1);
                register(target, Fankui, 1);
                register(target, Ganglie, 1);
                register(target, Yiji, amount as usize);
            }
            JudgeBefore { .. } => {
                for seat in self.living_order_from(self.active_seat) {
                    register(seat, Guicai, 1);
                }
            }
            JudgeAfter {
                seat, keep_black, ..
            } => {
                register(seat, Tiandu, 1);
                if keep_black {
                    register(seat, Luoshen, 1);
                }
            }
            ShaRedirect { target, .. } => register(target, Liuli, 1),
            ShaTarget { seat, .. } => register(seat, Tieji, 1),
            LostCards {
                seat,
                hand_count,
                equipment_count,
            } => {
                if hand_count > 0 && self.seat(seat).is_ok_and(|player| player.hand.is_empty()) {
                    register(seat, Lianying, 1);
                }
                register(seat, Xiaoji, equipment_count);
            }
            _ => {}
        }
        calls.reverse();
        calls
    }

    pub(super) fn resolve_hero_trigger(
        &mut self,
        trigger: Trigger,
        child: Option<Outcome<Resolution>>,
    ) {
        use SkillId::*;
        match trigger.skill {
            Biyue | Jizhi | Lianying | Xiaoji => self.resolve_hero_optional_draw(trigger, child),
            Guanxing => self.resolve_guanxing(trigger, child),
            Luoshen => self.resolve_luoshen(trigger, child),
            Tuxi | Yingzi | Luoyi | Keji => self.resolve_hero_phase_modifier(trigger, child),
            Jianxiong | Fankui | Ganglie | Yiji => self.resolve_hero_damage_trigger(trigger, child),
            Guicai | Tiandu => self.resolve_hero_judgment_trigger(trigger, child),
            Liuli | Tieji => self.resolve_hero_sha_trigger(trigger, child),
            _ => unreachable!("only registered skills can be queued"),
        }
    }

    fn continue_hero_trigger(&mut self, mut trigger: Trigger, stage: Stage) {
        trigger.stage = stage;
        self.scheduler
            .replace_current(Program::Hero(HeroFlow::Trigger(trigger)));
    }

    fn resolve_hero_optional_draw(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) {
        let owner = trigger.owner;
        let (name, amount) = match trigger.skill {
            SkillId::Biyue => ("闭月", 1),
            SkillId::Jizhi => ("集智", 1),
            SkillId::Lianying if self.seat(owner).is_ok_and(|player| player.hand.is_empty()) => {
                ("连营", 1)
            }
            SkillId::Xiaoji => ("枭姬", 2),
            SkillId::Lianying => {
                self.finish_hero(HeroAdjustment::None);
                return;
            }
            _ => unreachable!(),
        };
        if matches!(trigger.stage, Stage::Start) {
            self.continue_hero_trigger(trigger, Stage::Option);
            self.choose_hero_yes(owner, None, name);
        } else {
            if accepted(&child) {
                self.queue_draw(owner, amount, DrawReason::CardEffect);
            }
            self.finish_hero(HeroAdjustment::None);
        }
    }

    fn resolve_hero_phase_modifier(
        &mut self,
        trigger: Trigger,
        child: Option<Outcome<Resolution>>,
    ) {
        use SkillId::*;
        let owner = trigger.owner;
        let amount = match trigger.moment {
            HeroMoment::Draw { amount, .. } => amount,
            _ => 0,
        };
        let modifier = match trigger.stage {
            Stage::Start => {
                let branches = match trigger.skill {
                    Tuxi => {
                        let targets: Vec<_> = self
                            .living_order_from(owner)
                            .into_iter()
                            .skip(1)
                            .filter(|&target| {
                                self.seat(target).is_ok_and(|seat| !seat.hand.is_empty())
                            })
                            .collect();
                        if targets.is_empty() {
                            Vec::new()
                        } else {
                            vec![
                                option(0, "正常摸牌"),
                                branch(1, "突袭", vec![], (0, 0), targets, (1, 2)),
                            ]
                        }
                    }
                    Luoyi if amount > 0 => vec![option(0, "正常摸牌"), option(1, "裸衣")],
                    Yingzi if amount > 0 => vec![option(0, "正常摸牌"), option(1, "英姿")],
                    Keji if self
                        .classic
                        .as_ref()
                        .is_some_and(|state| !state.hero_turn.played_sha)
                        && self.seat(owner).is_ok_and(|seat| {
                            seat.hand.len() > usize::try_from(seat.hp).unwrap_or(0)
                        }) =>
                    {
                        vec![option(0, "正常弃牌"), option(1, "克己")]
                    }
                    _ => Vec::new(),
                };
                if !branches.is_empty() {
                    self.continue_hero_trigger(trigger, Stage::Option);
                    self.choose_hero(owner, None, branches);
                    return;
                }
                HeroAdjustment::None
            }
            _ => match selection(child).filter(|choice| choice.branch == 1) {
                Some(choice) => match trigger.skill {
                    Tuxi => {
                        for target in choice.targets {
                            if let Some(card) = self.random_hand_card(target) {
                                self.hero_transfer_card(target, Zone::Hand, card.id, owner);
                            }
                        }
                        HeroAdjustment::Draw(0)
                    }
                    Luoyi => {
                        self.classic.as_mut().expect("classic turn").hero_turn.luoyi = Some(owner);
                        HeroAdjustment::Draw(amount - 1)
                    }
                    Yingzi => HeroAdjustment::Draw(amount + 1),
                    Keji => HeroAdjustment::Skip,
                    _ => unreachable!(),
                },
                None => HeroAdjustment::None,
            },
        };
        self.finish_hero(modifier);
    }

    fn resolve_hero_damage_trigger(
        &mut self,
        trigger: Trigger,
        child: Option<Outcome<Resolution>>,
    ) {
        let HeroMoment::DamageAfter {
            source, ref cards, ..
        } = trigger.moment
        else {
            unreachable!()
        };
        let owner = trigger.owner;
        match trigger.skill {
            SkillId::Jianxiong => {
                let available: Vec<_> = cards
                    .iter()
                    .filter(|card| self.resolving.contains(card))
                    .copied()
                    .collect();
                if matches!(trigger.stage, Stage::Start) && !available.is_empty() {
                    self.continue_hero_trigger(trigger, Stage::Option);
                    self.choose_hero_yes(owner, source, "奸雄");
                    return;
                }
                if accepted(&child) {
                    for card in available {
                        let gained = self.take_resolving(card.id);
                        self.put_in_hand(owner, gained);
                    }
                }
            }
            SkillId::Fankui => {
                if let Some(source) =
                    source.filter(|seat| self.seat(*seat).is_ok_and(|player| player.alive))
                {
                    if self.resolve_fankui(trigger, source, child) {
                        return;
                    }
                }
            }
            SkillId::Ganglie => {
                if self.resolve_ganglie(trigger, source, child) {
                    return;
                }
            }
            SkillId::Yiji => {
                if self.resolve_yiji(trigger, child) {
                    return;
                }
            }
            _ => unreachable!(),
        }
        self.finish_hero(HeroAdjustment::None);
    }

    fn resolve_fankui(
        &mut self,
        trigger: Trigger,
        source: SeatId,
        child: Option<Outcome<Resolution>>,
    ) -> bool {
        let owner = trigger.owner;
        if matches!(trigger.stage, Stage::Start) {
            let opponent = self.seat(source).expect("the source exists");
            let mut options = vec![option(0, "放弃")];
            if source != owner && !opponent.hand.is_empty() {
                options.push(option(1, "反馈：随机获得手牌"));
            }
            if !opponent.equipment.is_empty() {
                options.push(branch(
                    2,
                    "反馈：获得装备",
                    opponent.equipment.iter().map(|card| card.id).collect(),
                    (1, 1),
                    vec![],
                    (0, 0),
                ));
            }
            if options.len() == 1 {
                return false;
            }
            self.continue_hero_trigger(trigger, Stage::Option);
            self.choose_hero(owner, Some(source), options);
            return true;
        }
        let Some(choice) = selection(child) else {
            return false;
        };
        if choice.branch == 1 {
            if let Some(card) = self.random_hand_card(source) {
                self.hero_transfer_card(source, Zone::Hand, card.id, owner);
            }
        } else if choice.branch == 2 {
            self.hero_transfer_card(source, Zone::Equipment, choice.cards[0], owner);
        }
        false
    }

    fn resolve_ganglie(
        &mut self,
        trigger: Trigger,
        source: Option<SeatId>,
        child: Option<Outcome<Resolution>>,
    ) -> bool {
        let owner = trigger.owner;
        match trigger.stage {
            Stage::Start => {
                self.continue_hero_trigger(trigger, Stage::Option);
                self.choose_hero_yes(owner, source, "刚烈");
            }
            Stage::Option if accepted(&child) => {
                self.continue_hero_trigger(trigger, Stage::Judge);
                self.scheduler.next(self.judge_program(owner, false));
            }
            Stage::Judge => {
                let Some(source) =
                    source.filter(|seat| self.seat(*seat).is_ok_and(|player| player.alive))
                else {
                    return false;
                };
                let Some(Outcome::Finished(Resolution::Judged(card))) = child else {
                    return false;
                };
                if self
                    .face(card.id)
                    .is_none_or(|face| face.suit == Suit::Heart)
                {
                    return false;
                }
                let hand = self.hero_owned_card_ids(source, false);
                if hand.len() < 2 {
                    self.next(EventKind::Damage {
                        source: Some(owner),
                        target: source,
                        amount: 1,
                    });
                    return false;
                }
                self.continue_hero_trigger(trigger, Stage::Discard);
                self.choose_hero(
                    source,
                    Some(owner),
                    vec![
                        option(0, "受到刚烈伤害"),
                        branch(1, "弃置两张手牌", hand, (2, 2), vec![], (0, 0)),
                    ],
                );
            }
            Stage::Discard => {
                let source = source.expect("only a living source receives the discard choice");
                if let Some(choice) = selection(child).filter(|choice| choice.branch == 1) {
                    self.hero_discard_cards(source, &choice.cards);
                } else {
                    self.next(EventKind::Damage {
                        source: Some(owner),
                        target: source,
                        amount: 1,
                    });
                }
                return false;
            }
            _ => return false,
        }
        true
    }

    fn resolve_yiji(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) -> bool {
        let owner = trigger.owner;
        match trigger.stage {
            Stage::Start => {
                self.continue_hero_trigger(trigger, Stage::Option);
                self.choose_hero_yes(owner, None, "遗计");
            }
            Stage::Option if accepted(&child) => {
                let Some(cards) = self.draw_from_piles(2) else {
                    return false;
                };
                self.classic
                    .as_mut()
                    .expect("the table is classic")
                    .hero_turn
                    .preview = cards;
                self.continue_hero_trigger(trigger, Stage::Distribute);
                self.choose_yiji(owner);
            }
            Stage::Distribute => {
                let choice = selection(child).expect("the distribution selection is required");
                for id in choice.cards {
                    let preview = &mut self
                        .classic
                        .as_mut()
                        .expect("the table is classic")
                        .hero_turn
                        .preview;
                    let card = Self::remove_card(preview, id);
                    self.put_in_hand(choice.targets[0], card);
                }
                if !self.choose_yiji(owner) {
                    return false;
                }
            }
            _ => return false,
        }
        true
    }

    fn choose_yiji(&mut self, owner: SeatId) -> bool {
        let preview = &self
            .classic
            .as_ref()
            .expect("the table is classic")
            .hero_turn
            .preview;
        if preview.is_empty() {
            return false;
        }
        self.choose_hero(
            owner,
            None,
            vec![branch(
                1,
                "遗计：选择牌和获得者",
                preview.iter().map(|card| card.id).collect(),
                (1, preview.len()),
                self.living_order_from(owner),
                (1, 1),
            )],
        );
        true
    }

    fn resolve_hero_judgment_trigger(
        &mut self,
        trigger: Trigger,
        child: Option<Outcome<Resolution>>,
    ) {
        let owner = trigger.owner;
        match trigger.moment {
            HeroMoment::JudgeBefore { seat, card } => {
                if matches!(trigger.stage, Stage::Start) {
                    let hand = self.hero_owned_card_ids(owner, false);
                    if !hand.is_empty() {
                        self.continue_hero_trigger(trigger, Stage::Option);
                        self.choose_hero(
                            owner,
                            Some(seat),
                            vec![
                                option(0, "保留判定"),
                                branch(1, "鬼才", hand, (1, 1), vec![], (0, 0)),
                            ],
                        );
                        return;
                    }
                } else if let Some(choice) = selection(child).filter(|choice| choice.branch == 1) {
                    let replacement = self.take_card(owner, Zone::Hand, choice.cards[0]);
                    self.resolving.push(replacement);
                    self.queue_hero_losses(owner, 1, 0);
                    self.next(EventKind::Discard { card });
                    self.next(EventKind::Reveal {
                        seat,
                        card: replacement,
                    });
                    self.finish_hero(HeroAdjustment::Judgment(replacement));
                    return;
                }
            }
            HeroMoment::JudgeAfter { card, .. } if self.resolving.contains(&card) => {
                if matches!(trigger.stage, Stage::Start) {
                    self.continue_hero_trigger(trigger, Stage::Option);
                    self.choose_hero_yes(owner, None, "天妒");
                    return;
                }
                if accepted(&child) {
                    let card = self.take_resolving(card.id);
                    self.put_in_hand(owner, card);
                }
            }
            _ => {}
        }
        self.finish_hero(HeroAdjustment::None);
    }

    fn resolve_guanxing(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) {
        let owner = trigger.owner;
        match trigger.stage {
            Stage::Start => {
                self.continue_hero_trigger(trigger, Stage::Option);
                self.choose_hero_yes(owner, None, "观星");
                return;
            }
            Stage::Option if accepted(&child) => {
                let amount = self.living_order_from(owner).len().min(5);
                if let Some(cards) = self.draw_from_piles(amount) {
                    let ids = cards.iter().map(|card| card.id).collect();
                    self.classic
                        .as_mut()
                        .expect("the table is classic")
                        .hero_turn
                        .preview = cards;
                    self.continue_hero_trigger(trigger, Stage::GuanxingTop);
                    self.choose_hero(
                        owner,
                        None,
                        vec![branch(
                            1,
                            "观星：按顺序放在牌堆顶",
                            ids,
                            (0, amount),
                            vec![],
                            (0, 0),
                        )],
                    );
                    return;
                }
            }
            Stage::GuanxingTop => {
                let top = selection(child).expect("the top order is required").cards;
                let preview = &self
                    .classic
                    .as_ref()
                    .expect("the table is classic")
                    .hero_turn
                    .preview;
                let bottom: Vec<_> = preview
                    .iter()
                    .filter(|card| !top.contains(&card.id))
                    .map(|card| card.id)
                    .collect();
                if bottom.len() <= 1 {
                    self.place_guanxing_cards(&top, &bottom);
                } else {
                    let count = bottom.len();
                    self.continue_hero_trigger(trigger, Stage::GuanxingBottom { top });
                    self.choose_hero(
                        owner,
                        None,
                        vec![branch(
                            1,
                            "观星：按顺序放在牌堆底",
                            bottom,
                            (count, count),
                            vec![],
                            (0, 0),
                        )],
                    );
                    return;
                }
            }
            Stage::GuanxingBottom { top } => {
                let bottom = selection(child)
                    .expect("the bottom order is required")
                    .cards;
                self.place_guanxing_cards(&top, &bottom);
            }
            _ => {}
        }
        self.finish_hero(HeroAdjustment::None);
    }

    fn place_guanxing_cards(&mut self, top: &[CardId], bottom: &[CardId]) {
        let state = self.classic.as_mut().expect("the table is classic");
        let preview = &mut state.hero_turn.preview;
        let bottom_cards: Vec<_> = bottom
            .iter()
            .rev()
            .map(|id| Self::remove_card(preview, *id))
            .collect();
        state.deck.splice(..0, bottom_cards);
        state
            .deck
            .extend(top.iter().rev().map(|id| Self::remove_card(preview, *id)));
    }

    fn resolve_luoshen(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) {
        let owner = trigger.owner;
        if let HeroMoment::JudgeAfter { card, .. } = trigger.moment {
            if self.hero_card_is_black(card) && self.resolving.contains(&card) {
                let card = self.take_resolving(card.id);
                self.put_in_hand(owner, card);
            }
        } else {
            match trigger.stage {
                Stage::Start => {
                    self.continue_hero_trigger(trigger, Stage::Option);
                    self.choose_hero_yes(owner, None, "洛神");
                    return;
                }
                Stage::Option | Stage::Repeat if accepted(&child) => {
                    self.continue_hero_trigger(trigger, Stage::Judge);
                    self.scheduler.next(self.judge_program(owner, true));
                    return;
                }
                Stage::Judge => {
                    if let Some(Outcome::Finished(Resolution::Judged(card))) = child {
                        if self.hero_card_is_black(card) && self.match_result().is_none() {
                            self.continue_hero_trigger(trigger, Stage::Repeat);
                            self.choose_hero_yes(owner, None, "再次发动洛神");
                            return;
                        }
                    }
                }
                _ => {}
            }
        }
        self.finish_hero(HeroAdjustment::None);
    }

    fn resolve_hero_sha_trigger(&mut self, trigger: Trigger, child: Option<Outcome<Resolution>>) {
        let owner = trigger.owner;
        let result = match &trigger.moment {
            HeroMoment::ShaTarget { target, .. } => {
                let target = *target;
                match trigger.stage {
                    Stage::Start => {
                        self.continue_hero_trigger(trigger, Stage::Option);
                        self.choose_hero_yes(owner, Some(target), "铁骑");
                        return;
                    }
                    Stage::Option if accepted(&child) => {
                        self.continue_hero_trigger(trigger, Stage::Judge);
                        self.scheduler.next(self.judge_program(owner, false));
                        return;
                    }
                    Stage::Judge if matches!(child, Some(Outcome::Finished(Resolution::Judged(card))) if self.hero_card_is_red(card)) => {
                        HeroAdjustment::DirectHit
                    }
                    _ => HeroAdjustment::None,
                }
            }
            HeroMoment::ShaRedirect {
                seat,
                target,
                excluded,
            } => {
                let attacker = *seat;
                let original = *target;
                if matches!(trigger.stage, Stage::Start) {
                    let targets: Vec<_> = self
                        .living_order_from(original)
                        .into_iter()
                        .skip(1)
                        .filter(|&target| {
                            target != attacker
                                && !excluded.contains(&target)
                                && self.hero_allows_target(target, CardKind::Sha)
                        })
                        .collect();
                    let mut branches = vec![option(0, "放弃")];
                    for cost in self.hero_owned_card_ids(owner, true) {
                        let choices: Vec<_> = targets
                            .iter()
                            .copied()
                            .filter(|&target| self.in_sha_range_without(owner, target, Some(cost)))
                            .collect();
                        if choices.is_empty() {
                            continue;
                        }
                        if let Some(existing) =
                            branches.iter_mut().find(|branch| branch.targets == choices)
                        {
                            existing.cards.push(cost);
                        } else {
                            branches.push(branch(
                                branches.len() as u32,
                                "流离",
                                vec![cost],
                                (1, 1),
                                choices,
                                (1, 1),
                            ));
                        }
                    }
                    if branches.len() > 1 {
                        self.continue_hero_trigger(trigger, Stage::Option);
                        self.choose_hero(owner, Some(attacker), branches);
                        return;
                    }
                    HeroAdjustment::None
                } else if let Some(choice) = selection(child).filter(|choice| choice.branch != 0) {
                    self.hero_discard_cards(owner, &choice.cards);
                    HeroAdjustment::Redirect(choice.targets[0])
                } else {
                    HeroAdjustment::None
                }
            }
            _ => unreachable!(),
        };
        self.finish_hero(result);
    }
}

fn accepted(child: &Option<Outcome<Resolution>>) -> bool {
    matches!(child, Some(Outcome::Finished(Resolution::Selected(choice))) if choice.branch == 1)
}
