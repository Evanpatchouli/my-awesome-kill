use super::{branch, Flow};
use crate::classic::skills::HeroMoment;
use crate::classic_data::Suit;
use crate::scheduler::Outcome;
use crate::{Card, CardKind, ChoiceBranch, EventKind, Game, Program, Resolution, SeatId};

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

#[derive(Clone, Copy)]
pub(crate) struct Response {
    pub(super) seat: SeatId,
    pub(super) kind: CardKind,
    ignore_armor: bool,
    allow_lord: bool,
    step: Step,
}

impl Response {
    pub(super) fn new(seat: SeatId, kind: CardKind, ignore_armor: bool) -> Self {
        Self {
            seat,
            kind,
            ignore_armor,
            allow_lord: true,
            step: Step::Start,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Payment {
    seat: SeatId,
    kind: CardKind,
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
        let mut response = Response::new(seat, kind, false);
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
                    self.sha_use_branches(seat, &[])
                } else {
                    self.response_card_branches(seat, kind)
                };
                self.append_lord_branch(response, &mut branches);
                if kind == CardKind::Wuxie && branches.len() == 1 {
                    self.scheduler.end(Finished(Passed));
                } else {
                    self.choose_response(response, branches);
                }
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
            (Step::Hand, Some(Finished(Dodged)))
            | (Step::Pay, Some(Finished(_)))
            | (Step::Lord, Some(Finished(Responded))) => self.scheduler.end(Finished(Responded)),
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

    fn choose_response(&mut self, mut response: Response, branches: Vec<ChoiceBranch>) {
        response.step = Step::Choose;
        self.continue_equipment(Flow::Response(response));
        self.choose_equipment(response.seat, None, branches);
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
                cards,
            })));
    }

    pub(super) fn resolve_response_payment(&mut self, payment: Payment) {
        if payment.kind == CardKind::Sha {
            self.note_hero_sha_play(payment.seat);
        }
        self.queue_use_costs(payment.seat, &payment.cards);
        if payment.kind == CardKind::Wuxie {
            self.queue_hero_moment(HeroMoment::UseTrick {
                seat: payment.seat,
                kind: payment.kind,
            });
        }
        for card in payment.cards {
            self.scheduler
                .after(Program::event(EventKind::Discard { card }));
        }
        self.finish();
    }
}
