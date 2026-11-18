use super::{branch, Flow};
use crate::classic::skills::HeroMoment;
use crate::classic_data::Suit;
use crate::scheduler::Outcome;
use crate::{
    Card, CardKind, ChoiceBranch, ChoiceRequest, CounterContext, EventKind, Game, Program,
    Resolution, SeatId,
};

#[derive(Clone, Copy)]
enum Step {
    Start,
    OfferArmor,
    Judge,
    Hand,
    Choose,
    Lord,
    Pay,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseContext {
    Use,
    Respond,
}

#[derive(Clone, Copy)]
pub(crate) struct Response {
    pub(super) seat: SeatId,
    pub(super) kind: CardKind,
    ignore_armor: bool,
    pub(super) context: ResponseContext,
    allow_lord: bool,
    step: Step,
    counter: Option<CounterContext>,
}

impl Response {
    pub(super) fn new(seat: SeatId, kind: CardKind, ignore_armor: bool) -> Self {
        Self {
            seat,
            kind,
            ignore_armor,
            context: ResponseContext::Use,
            allow_lord: true,
            step: Step::Start,
            counter: None,
        }
    }

    pub(super) fn respond(seat: SeatId, kind: CardKind) -> Self {
        Self {
            context: ResponseContext::Respond,
            ..Self::new(seat, kind, false)
        }
    }

    pub(super) fn counter(seat: SeatId, context: CounterContext) -> Self {
        Self {
            counter: Some(context),
            ..Self::new(seat, CardKind::Wuxie, false)
        }
    }
}

#[derive(Clone, Copy)]
enum PaymentStep {
    Pay,
    Timing,
    Finish,
}

#[derive(Clone)]
pub(crate) struct Payment {
    seat: SeatId,
    kind: CardKind,
    context: ResponseContext,
    step: PaymentStep,
    cards: Vec<Card>,
}

impl Payment {
    pub(super) fn event(&self) -> EventKind {
        match self.cards.as_slice() {
            [card] if card.kind == self.kind => EventKind::UseResponseCard {
                seat: self.seat,
                card: *card,
            },
            _ => EventKind::UseVirtual {
                seat: self.seat,
                kind: self.kind,
            },
        }
    }
}

impl Game {
    pub(crate) fn forbid_lord_request_program(&self, seat: SeatId, kind: CardKind) -> Program {
        let mut response = Response::respond(seat, kind);
        response.allow_lord = false;
        Program::Equipment(Flow::Response(response))
    }

    pub(super) fn resolve_response(
        &mut self,
        mut response: Response,
        child: Option<Outcome<Resolution>>,
    ) {
        use Outcome::{Cancelled, Finished};
        use Resolution::{Dodged, Judged, Passed, Responded, Selected};
        if self.match_result().is_some() {
            self.scheduler.end(Cancelled);
            return;
        }
        let seat = response.seat;
        let kind = response.kind;
        if !self.is_alive(seat) {
            self.scheduler.end(Finished(Passed));
            return;
        }
        if matches!(
            response.step,
            Step::Start | Step::OfferArmor | Step::Judge | Step::Choose | Step::Lord
        ) && response.context == ResponseContext::Use
            && !self.card_enabled(seat, kind)
        {
            self.scheduler.end(Finished(Passed));
            return;
        }
        match (response.step, child) {
            (Step::Start, None) if kind == CardKind::Shan => {
                let armor = self
                    .seat(seat)
                    .expect("the responding seat exists")
                    .equipment
                    .iter()
                    .any(|card| card.kind == CardKind::Bagua);
                if armor && !response.ignore_armor {
                    response.step = Step::OfferArmor;
                    self.continue_equipment(Flow::Response(response));
                    self.choose_equipment(
                        seat,
                        None,
                        vec![
                            branch(0, "继续选择闪", vec![], 0),
                            branch(1, "发动八卦阵", vec![], 0),
                        ],
                    );
                } else {
                    self.response_hand_shan(response);
                }
            }
            (Step::Start, None) => {
                let mut branches = if kind == CardKind::Sha {
                    self.sha_response_branches(seat, &[])
                } else {
                    self.response_card_branches(seat, kind)
                };
                self.append_lord_branch(response, &mut branches);
                self.choose_response(response, branches);
            }
            (Step::OfferArmor, Some(Finished(Selected(selection)))) => {
                if selection.branch == 1 {
                    response.step = Step::Judge;
                    self.continue_equipment(Flow::Response(response));
                    self.scheduler.next(self.judge_program(seat, false));
                } else {
                    self.response_hand_shan(response);
                }
            }
            (Step::Judge, Some(Finished(Judged(judgment)))) => {
                if self
                    .face(judgment.id)
                    .is_some_and(|face| matches!(face.suit, Suit::Heart | Suit::Diamond))
                {
                    self.response_pay(response, vec![]);
                } else {
                    self.response_hand_shan(response);
                }
            }
            (Step::Judge, Some(Cancelled)) => self.scheduler.end(Cancelled),
            (Step::Hand, Some(Finished(Dodged))) | (Step::Pay, Some(Finished(_))) => {
                self.scheduler.end(Finished(Responded))
            }
            (Step::Hand | Step::Pay, Some(Cancelled)) => self.scheduler.end(Finished(Passed)),
            (Step::Lord, Some(Finished(Responded))) if response.context == ResponseContext::Use => {
                self.response_pay(response, vec![])
            }
            (Step::Lord, Some(Finished(Responded))) => self.scheduler.end(Finished(Responded)),
            (Step::Hand, Some(Finished(Passed))) => self.scheduler.end(Finished(Passed)),
            (Step::Choose, Some(Finished(Selected(selection)))) => match selection.branch {
                0 => self.scheduler.end(Finished(Passed)),
                99 => {
                    let request = self
                        .hero_lord_request(seat, kind, vec![])
                        .expect("the selected lord request is available");
                    response.step = Step::Lord;
                    self.continue_equipment(Flow::Response(response));
                    self.scheduler.next(request);
                }
                _ => self.response_pay(response, self.hero_cards_by_id(seat, &selection.cards)),
            },
            (Step::Lord, Some(_)) => {
                response.allow_lord = false;
                if kind == CardKind::Shan {
                    self.response_hand_shan(response);
                } else {
                    response.step = Step::Start;
                    self.continue_equipment(Flow::Response(response));
                }
            }
            _ => unreachable!("a response waits for its selection, judgment, or payment"),
        }
    }

    fn append_lord_branch(&self, response: Response, branches: &mut Vec<ChoiceBranch>) {
        if response.allow_lord && self.hero_lord_request_available(response.seat, response.kind) {
            let name = if response.kind == CardKind::Shan {
                "护驾"
            } else {
                "激将"
            };
            branches.push(branch(99, name, vec![], 0));
        }
    }

    fn response_card_branches(&self, seat: SeatId, kind: CardKind) -> Vec<ChoiceBranch> {
        let actor = self.seat(seat).expect("the responding seat exists");
        let cards: Vec<_> = actor
            .hand
            .iter()
            .filter(|card| card.kind == kind)
            .map(|card| card.id)
            .collect();
        let mut branches = vec![branch(0, "放弃", vec![], 0)];
        if !cards.is_empty() {
            let label = match kind {
                CardKind::Wuxie => "使用无懈可击",
                CardKind::Shan => "使用闪",
                _ => "打出响应牌",
            };
            branches.push(branch(1, label, cards, 1));
        }
        branches
    }

    fn choose_response(&mut self, mut response: Response, mut branches: Vec<ChoiceBranch>) {
        response.step = Step::Choose;
        self.continue_equipment(Flow::Response(response));
        if let Some(context) = response.counter {
            if let Some(play) = branches.iter_mut().find(|branch| branch.id == 1) {
                play.label = if context.negated {
                    "无懈可击：恢复原效果"
                } else {
                    "无懈可击：抵消原效果"
                }
                .into();
            }
            self.scheduler.next(Program::CounterChoice {
                request: ChoiceRequest {
                    seat: response.seat,
                    target: Some(context.target),
                    branches,
                },
                context,
            });
        } else {
            self.choose_equipment(response.seat, None, branches);
        }
    }

    fn response_hand_shan(&mut self, mut response: Response) {
        let mut branches = self.hero_response_conversions(response.seat, CardKind::Shan);
        self.append_lord_branch(response, &mut branches);
        if branches.is_empty() {
            response.step = Step::Hand;
            self.continue_equipment(Flow::Response(response));
            self.next(EventKind::ChooseShan {
                seat: response.seat,
            });
        } else {
            let mut all = self.response_card_branches(response.seat, CardKind::Shan);
            all.append(&mut branches);
            self.choose_response(response, all);
        }
    }

    fn response_pay(&mut self, mut response: Response, cards: Vec<Card>) {
        response.step = Step::Pay;
        self.continue_equipment(Flow::Response(response));
        self.scheduler
            .next(Program::Equipment(Flow::Payment(Payment {
                seat: response.seat,
                kind: response.kind,
                context: response.context,
                step: PaymentStep::Pay,
                cards,
            })));
    }

    pub(super) fn resolve_response_payment(
        &mut self,
        mut payment: Payment,
        child: Option<Outcome<Resolution>>,
    ) {
        match payment.step {
            PaymentStep::Pay => {
                if payment.kind == CardKind::Sha {
                    self.note_hero_sha_play(payment.seat);
                }
                self.queue_use_costs(payment.seat, &payment.cards);
                for &card in &payment.cards {
                    self.scheduler
                        .after(Program::event(EventKind::Discard { card }));
                }
                payment.step = PaymentStep::Timing;
                self.scheduler
                    .replace_current(Program::Equipment(Flow::Payment(payment)));
            }
            PaymentStep::Timing => {
                payment.step = PaymentStep::Finish;
                self.scheduler
                    .replace_current(Program::Equipment(Flow::Payment(payment.clone())));
                if payment.context == ResponseContext::Use {
                    self.queue_use_card2(payment.seat, payment.kind, &payment.cards);
                }
            }
            PaymentStep::Finish => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                if payment.kind == CardKind::Wuxie {
                    self.queue_hero_moment(HeroMoment::UseTrick {
                        seat: payment.seat,
                        kind: payment.kind,
                        cards: payment.cards,
                    });
                }
                self.finish();
            }
        }
    }
}
