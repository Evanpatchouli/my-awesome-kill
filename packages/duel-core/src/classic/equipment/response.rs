use super::{branch, Flow};
use crate::classic_data::Suit;
use crate::scheduler::Outcome;
use crate::{Card, CardKind, EventKind, Game, Program, Resolution, SeatId};

#[derive(Clone, Copy)]
enum Step {
    Start,
    OfferArmor,
    Judge,
    Hand,
    Choose,
    Pay,
}

#[derive(Clone, Copy)]
pub(crate) struct Response {
    pub(super) seat: SeatId,
    pub(super) kind: CardKind,
    ignore_armor: bool,
    step: Step,
}

impl Response {
    pub(super) fn new(seat: SeatId, kind: CardKind, ignore_armor: bool) -> Self {
        Self {
            seat,
            kind,
            ignore_armor,
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
        if let [card] = self.cards.as_slice() {
            EventKind::UseResponseCard {
                seat: self.seat,
                card: *card,
            }
        } else {
            EventKind::UseVirtual {
                seat: self.seat,
                kind: self.kind,
            }
        }
    }
}

impl Game {
    pub(super) fn resolve_response(
        &mut self,
        mut response: Response,
        child: Option<Outcome<Resolution>>,
    ) {
        use Outcome::{Cancelled, Finished};
        use Resolution::{Dodged, Judged, Passed, Responded, Selected};
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
                            branch(0, "使用手牌闪", vec![], 0),
                            branch(1, "发动八卦阵", vec![], 0),
                        ],
                    );
                } else {
                    self.response_hand_shan(response);
                }
            }
            (Step::Start, None) => {
                let branches = if kind == CardKind::Sha {
                    self.sha_use_branches(seat, &[])
                } else {
                    let actor = self.seat(seat).expect("the responding seat exists");
                    let cards: Vec<_> = actor
                        .hand
                        .iter()
                        .filter(|card| card.kind == kind)
                        .map(|card| card.id)
                        .collect();
                    let mut branches = vec![branch(0, "放弃", vec![], 0)];
                    if !cards.is_empty() {
                        let label = if kind == CardKind::Wuxie {
                            "使用无懈可击"
                        } else {
                            "打出响应牌"
                        };
                        branches.push(branch(1, label, cards, 1));
                    }
                    branches
                };
                if kind == CardKind::Wuxie && branches.len() == 1 {
                    self.scheduler.end(Finished(Passed));
                    return;
                }
                response.step = Step::Choose;
                self.continue_equipment(Flow::Response(response));
                self.choose_equipment(seat, None, branches);
            }
            (Step::OfferArmor, Some(Finished(Selected(selection)))) => {
                if selection.branch == 1 {
                    response.step = Step::Judge;
                    self.continue_equipment(Flow::Response(response));
                    self.scheduler.next(Program::Judge { seat });
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
            (Step::Hand, Some(Finished(Passed))) => self.scheduler.end(Finished(Passed)),
            (Step::Choose, Some(Finished(Selected(selection)))) => {
                if selection.branch == 0 {
                    self.scheduler.end(Finished(Passed));
                } else {
                    self.response_pay(response, self.hand_cards_by_id(seat, &selection.cards));
                }
            }
            _ => unreachable!("a response waits for its selection, judgment, or payment"),
        }
    }

    fn response_hand_shan(&mut self, mut response: Response) {
        response.step = Step::Hand;
        self.continue_equipment(Flow::Response(response));
        self.next(EventKind::ChooseShan {
            seat: response.seat,
        });
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
        for card in payment.cards {
            self.next(EventKind::UseCost {
                seat: payment.seat,
                card,
            });
            self.scheduler
                .after(Program::event(EventKind::Discard { card }));
        }
        self.finish();
    }
}
