use super::{branch, option, selection, HeroFlow, SkillId};
use crate::classic_data::Suit;
use crate::scheduler::Outcome;
use crate::{
    Action, CardId, CardKind, DrawReason, EventKind, Game, Phase, Program, Resolution, RuleError,
    SeatId, SkillPlay, Zone,
};

#[derive(Clone)]
pub(crate) struct Active {
    seat: SeatId,
    skill: SkillId,
    cards: Vec<CardId>,
    targets: Vec<SeatId>,
    stage: ActiveStage,
}

#[derive(Clone, Copy)]
enum ActiveStage {
    Begin,
    Suit,
    DuelSource,
    HealthLost,
}

impl Active {
    pub(super) fn kind(&self) -> EventKind {
        EventKind::Skill {
            seat: self.seat,
            skill: self.skill,
        }
    }
}

#[derive(Clone)]
pub(crate) struct LordRequest {
    lord: SeatId,
    kind: CardKind,
    targets: Vec<SeatId>,
    applicants: Vec<SeatId>,
    stage: RequestStage,
}

#[derive(Clone, Copy)]
enum RequestStage {
    Next,
    Response,
    PlayChoice(SeatId),
}

impl LordRequest {
    pub(super) fn kind(&self) -> EventKind {
        EventKind::Skill {
            seat: self.lord,
            skill: if self.kind == CardKind::Shan {
                SkillId::Hujia
            } else {
                SkillId::Jijiang
            },
        }
    }
}

impl Game {
    pub(crate) fn hero_skill_plays(&self, seat: SeatId) -> Vec<SkillPlay> {
        use SkillId::*;
        let Some(state) = &self.classic else {
            return Vec::new();
        };
        if !self.is_in(seat) {
            return Vec::new();
        }
        let living = self.in_order_from(seat);
        let others = living.iter().skip(1).copied().collect::<Vec<_>>();
        let hand = self.hero_owned_card_ids(seat, false);
        let owned = self.hero_owned_card_ids(seat, true);
        let wounded = |seat| self.seat(seat).is_ok_and(|seat| seat.hp < seat.max_hp);
        let male = |seat| {
            self.seat(seat)
                .ok()
                .and_then(|s| s.hero)
                .is_some_and(|hero| hero.gender == "male")
        };
        let mut plays = Vec::new();
        let mut offer = |skill: SkillId,
                         name,
                         cards: &[CardId],
                         min_cards,
                         max_cards,
                         targets: Vec<SeatId>,
                         count| {
            if self.has_skill(seat, skill.as_str())
                && !state.hero_turn.used.contains(&skill)
                && cards.len() >= min_cards
                && targets.len() >= count
            {
                let id = plays.len() as u32;
                plays.push(SkillPlay {
                    skill,
                    branch: branch(
                        id,
                        name,
                        cards.to_vec(),
                        (min_cards, max_cards),
                        targets,
                        (count, count),
                    ),
                });
            }
        };
        offer(Rende, "仁德", &hand, 1, hand.len(), others.clone(), 1);
        offer(Zhiheng, "制衡", &owned, 1, owned.len(), vec![], 0);
        offer(Kurou, "苦肉", &[], 0, 0, vec![], 0);
        if !hand.is_empty() {
            offer(Fanjian, "反间", &[], 0, 0, others.clone(), 1);
        }
        offer(
            Jieyin,
            "结姻",
            &hand,
            2,
            2,
            others
                .iter()
                .copied()
                .filter(|&target| male(target) && wounded(target))
                .collect(),
            1,
        );
        offer(
            Qingnang,
            "青囊",
            &hand,
            1,
            1,
            living
                .iter()
                .copied()
                .filter(|&target| wounded(target))
                .collect(),
            1,
        );
        if others
            .iter()
            .copied()
            .filter(|&target| male(target))
            .count()
            >= 2
        {
            offer(
                Lijian,
                "离间：选择先响应决斗的角色",
                &owned,
                1,
                1,
                others
                    .iter()
                    .copied()
                    .filter(|&target| {
                        male(target)
                            && self.hero_allows_target(target, CardKind::Juedou)
                            && others.iter().copied().any(|source| {
                                source != target
                                    && male(source)
                                    && self.card_enabled(source, CardKind::Juedou)
                            })
                    })
                    .collect(),
                1,
            );
        }
        if self.has_lord_skill(seat, Jijiang.as_str())
            && self.may_play_sha(seat)
            && self.card_enabled(seat, CardKind::Sha)
            && self.hero_lord_request_available(seat, CardKind::Sha)
        {
            let targets = others
                .into_iter()
                .filter(|&target| {
                    self.in_sha_range(seat, target)
                        && self.hero_allows_target(target, CardKind::Sha)
                })
                .collect();
            offer(Jijiang, "激将", &[], 0, 0, targets, 1);
        }
        plays
    }

    pub(crate) fn plan_hero_skill(
        &self,
        seat: SeatId,
        skill: SkillId,
        cards: Vec<CardId>,
        targets: Vec<SeatId>,
    ) -> Result<Program, RuleError> {
        self.checked_play_seat(seat)?;
        if !self
            .hero_skill_plays(seat)
            .iter()
            .any(|play| play.skill == skill && Self::matches_branch(&play.branch, &cards, &targets))
        {
            return Err(RuleError::InvalidSelection);
        }
        if skill == SkillId::Jijiang {
            return self
                .hero_lord_request(seat, CardKind::Sha, targets)
                .ok_or(RuleError::InvalidSelection);
        }
        Ok(Program::Hero(HeroFlow::Active(Active {
            seat,
            skill,
            cards,
            targets,
            stage: ActiveStage::Begin,
        })))
    }

    pub(super) fn resolve_hero_active(
        &mut self,
        mut active: Active,
        child: Option<Outcome<Resolution>>,
    ) {
        use SkillId::*;
        let seat = active.seat;
        if self.match_result().is_some() {
            self.finish();
            return;
        }
        match active.stage {
            ActiveStage::Begin => {
                if !self.is_in(seat) || active.targets.iter().any(|&target| !self.is_in(target)) {
                    self.finish();
                    return;
                }
                if matches!(active.skill, Zhiheng | Fanjian | Jieyin | Qingnang | Lijian) {
                    self.classic
                        .as_mut()
                        .expect("the table is classic")
                        .hero_turn
                        .used
                        .push(active.skill);
                }
                match active.skill {
                    Rende => {
                        let mut given = Vec::with_capacity(active.cards.len());
                        for id in &active.cards {
                            let card = self.take_card(seat, Zone::Hand, *id);
                            self.put_in_hand(active.targets[0], card);
                            given.push((Zone::Hand, card));
                        }
                        self.queue_hero_losses(seat, given);
                        let turn = &mut self
                            .classic
                            .as_mut()
                            .expect("the table is classic")
                            .hero_turn;
                        let before = turn.rende_given;
                        turn.rende_given += active.cards.len();
                        if before < 2 && turn.rende_given >= 2 {
                            self.hero_recover(seat, seat);
                        }
                    }
                    Zhiheng => {
                        self.hero_discard_cards(seat, &active.cards);
                        self.queue_draw(seat, active.cards.len() as u16, DrawReason::CardEffect);
                    }
                    Kurou => {
                        active.stage = ActiveStage::HealthLost;
                        self.scheduler
                            .replace_current(Program::Hero(HeroFlow::Active(active)));
                        self.queue_lose_hp(seat, 1);
                        return;
                    }
                    Fanjian => {
                        let target = active.targets[0];
                        active.stage = ActiveStage::Suit;
                        self.scheduler
                            .replace_current(Program::Hero(HeroFlow::Active(active)));
                        self.choose_hero(
                            target,
                            Some(seat),
                            vec![
                                option(0, "黑桃"),
                                option(1, "梅花"),
                                option(2, "红桃"),
                                option(3, "方片"),
                            ],
                        );
                        return;
                    }
                    Jieyin => {
                        self.hero_discard_cards(seat, &active.cards);
                        self.hero_recover(seat, seat);
                        self.hero_recover(seat, active.targets[0]);
                    }
                    Qingnang => {
                        self.hero_discard_cards(seat, &active.cards);
                        self.hero_recover(seat, active.targets[0]);
                    }
                    Lijian => {
                        let target = active.targets[0];
                        let sources = self
                            .in_order_from(seat)
                            .into_iter()
                            .skip(1)
                            .filter(|&candidate| {
                                candidate != target
                                    && self.card_enabled(candidate, CardKind::Juedou)
                                    && self
                                        .seat(candidate)
                                        .ok()
                                        .and_then(|seat| seat.hero)
                                        .is_some_and(|hero| hero.gender == "male")
                            })
                            .collect();
                        active.stage = ActiveStage::DuelSource;
                        self.scheduler
                            .replace_current(Program::Hero(HeroFlow::Active(active)));
                        self.choose_hero(
                            seat,
                            Some(target),
                            vec![branch(
                                1,
                                "离间：选择决斗的使用者",
                                vec![],
                                (0, 0),
                                sources,
                                (1, 1),
                            )],
                        );
                        return;
                    }
                    _ => unreachable!("only active skills are planned for play"),
                }
            }
            ActiveStage::Suit => {
                let Some(choice) = selection(child) else {
                    self.finish();
                    return;
                };
                if !self.is_in(seat) || !self.is_in(active.targets[0]) {
                    self.finish();
                    return;
                }
                let suit =
                    [Suit::Spade, Suit::Club, Suit::Heart, Suit::Diamond][choice.branch as usize];
                if let Some(card) = self.random_hand_card(seat) {
                    let target = active.targets[0];
                    self.hero_transfer_card(seat, Zone::Hand, card.id, target);
                    if self.face(card.id).is_some_and(|face| face.suit != suit) {
                        self.next(EventKind::Damage {
                            source: Some(seat),
                            target,
                            amount: 1,
                        });
                    }
                }
            }
            ActiveStage::DuelSource => {
                let Some(choice) = selection(child) else {
                    self.finish();
                    return;
                };
                if !self.is_in(seat)
                    || !self.is_in(choice.targets[0])
                    || !self.is_in(active.targets[0])
                {
                    self.finish();
                    return;
                }
                self.hero_discard_cards(seat, &active.cards);
                self.scheduler
                    .next(self.virtual_duel_without_counter(choice.targets[0], active.targets[0]));
            }
            ActiveStage::HealthLost => {
                if self.is_in(seat) {
                    self.queue_draw(seat, 2, DrawReason::CardEffect);
                }
            }
        }
        self.finish();
    }

    fn hero_recover(&mut self, seat: SeatId, target: SeatId) {
        self.next(EventKind::Recover {
            seat,
            target,
            amount: 1,
        });
    }

    pub(crate) fn hero_lord_request_available(&self, seat: SeatId, kind: CardKind) -> bool {
        self.hero_lord_request(seat, kind, Vec::new()).is_some()
    }

    pub(crate) fn note_hero_non_jijiang_play(&mut self, seat: SeatId) {
        if let Some(classic) = self
            .classic
            .as_mut()
            .filter(|state| state.turn.is_some_and(|turn| turn.seat == seat))
        {
            classic
                .hero_turn
                .used
                .retain(|skill| *skill != SkillId::Jijiang);
        }
    }

    pub(crate) fn hero_lord_request(
        &self,
        seat: SeatId,
        kind: CardKind,
        targets: Vec<SeatId>,
    ) -> Option<Program> {
        let (skill, faction) = match kind {
            CardKind::Shan => (SkillId::Hujia, "wei"),
            CardKind::Sha => (SkillId::Jijiang, "shu"),
            _ => return None,
        };
        if !self.is_in(seat) || !self.has_lord_skill(seat, skill.as_str()) {
            return None;
        }
        let mut applicants: Vec<_> = self
            .in_order_from(seat)
            .into_iter()
            .skip(1)
            .filter(|&player| self.hero_faction_is(player, faction))
            .collect();
        if applicants.is_empty() {
            return None;
        }
        applicants.reverse();
        Some(Program::Hero(HeroFlow::LordRequest(LordRequest {
            lord: seat,
            kind,
            targets,
            applicants,
            stage: RequestStage::Next,
        })))
    }

    pub(super) fn resolve_hero_lord_request(
        &mut self,
        mut request: LordRequest,
        child: Option<Outcome<Resolution>>,
    ) {
        use RequestStage::*;
        if !self.is_in(request.lord) || request.targets.iter().any(|&seat| !self.is_in(seat)) {
            self.scheduler.end(Outcome::Finished(Resolution::Passed));
            return;
        }
        match request.stage {
            Response if matches!(child, Some(Outcome::Finished(Resolution::Responded))) => {
                if request.kind == CardKind::Sha {
                    self.note_hero_sha_play(request.lord);
                }
                self.scheduler.end(Outcome::Finished(Resolution::Responded));
                return;
            }
            PlayChoice(supplier) if self.is_in(supplier) => {
                if let Some(choice) = selection(child).filter(|choice| choice.branch != 0) {
                    let cards = self.hero_cards_by_id(supplier, &choice.cards);
                    let attack = self.delegated_sha_program(
                        request.lord,
                        supplier,
                        cards,
                        request.targets.clone(),
                    );
                    self.scheduler.next(attack);
                    self.finish();
                    return;
                }
            }
            _ => {}
        }
        if self.match_result().is_some() {
            self.scheduler.end(Outcome::Finished(Resolution::Passed));
            return;
        }
        while let Some(supplier) = request.applicants.pop() {
            if !self.is_in(supplier) {
                continue;
            }
            if request.targets.is_empty() {
                let response = self.forbid_lord_request_program(supplier, request.kind);
                request.stage = Response;
                self.scheduler
                    .replace_current(Program::Hero(HeroFlow::LordRequest(request)));
                self.scheduler.next(response);
                return;
            }
            let mut options = self.sha_response_branches(supplier, &[]);
            options.retain(|choice| choice.id != 99);
            if options.len() > 1 {
                let lord = request.lord;
                request.stage = PlayChoice(supplier);
                self.scheduler
                    .replace_current(Program::Hero(HeroFlow::LordRequest(request)));
                self.choose_hero(supplier, Some(lord), options);
                return;
            }
        }
        if !request.targets.is_empty() {
            if let Some(classic) = self.classic.as_mut().filter(|state| {
                state
                    .turn
                    .is_some_and(|turn| turn.seat == request.lord && turn.phase == Phase::Play)
            }) {
                classic.hero_turn.used.push(SkillId::Jijiang);
            }
        }
        self.scheduler.end(Outcome::Finished(Resolution::Passed));
    }

    pub(crate) fn bot_hero_play(&self, seat: SeatId) -> Option<Action> {
        let actor = self.seat(seat).ok()?;
        let plays = self.hero_skill_plays(seat);
        let mut candidates = plays
            .into_iter()
            .filter_map(|play| {
                let targets = &play.branch.targets;
                let score = match play.skill {
                    SkillId::Qingnang if targets.contains(&seat) => 8,
                    SkillId::Jieyin if actor.hp < actor.max_hp => 7,
                    SkillId::Fanjian => 5,
                    SkillId::Lijian => 4,
                    SkillId::Kurou if actor.hp >= 3 && actor.hand.len() < actor.hp as usize - 1 => {
                        4
                    }
                    SkillId::Zhiheng if actor.hand.len() >= 2 => 2,
                    _ => return None,
                };
                Some((score, play))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(score, _)| *score);
        let (_, play) = candidates.pop()?;
        let target = if play.skill == SkillId::Qingnang {
            Some(seat)
        } else {
            play.branch.targets.first().copied()
        };
        let cards = play
            .branch
            .cards
            .into_iter()
            .take(play.branch.min_cards)
            .collect();
        Some(Action::UseSkill {
            skill: play.skill,
            cards,
            targets: target.into_iter().collect(),
        })
    }
}
