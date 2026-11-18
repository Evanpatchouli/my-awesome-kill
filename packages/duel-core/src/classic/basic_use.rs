use crate::scheduler::Outcome;
use crate::{Card, CardKind, EventKind, Game, Program, Resolution, SeatId, Zone};

#[derive(Clone, Copy)]
enum Step {
    Pay,
    Timing,
    Resolve(usize),
}

#[derive(Clone)]
pub(crate) struct Flow {
    seat: SeatId,
    card: Card,
    kind: CardKind,
    virtual_card: bool,
    use_card: bool,
    announced_target: Option<SeatId>,
    targets: Vec<SeatId>,
    step: Step,
}

impl Flow {
    pub(crate) fn tao(seat: SeatId, target: SeatId, card: Card, virtual_card: bool) -> Self {
        Self {
            seat,
            card,
            kind: CardKind::Tao,
            virtual_card,
            use_card: true,
            announced_target: Some(target),
            targets: vec![target],
            step: Step::Pay,
        }
    }

    pub(crate) fn shan(seat: SeatId, card: Card, use_card: bool) -> Self {
        Self {
            seat,
            card,
            kind: CardKind::Shan,
            virtual_card: false,
            use_card,
            announced_target: None,
            targets: vec![],
            step: Step::Pay,
        }
    }

    pub(crate) fn kind(&self) -> EventKind {
        if self.virtual_card {
            EventKind::UseVirtual {
                seat: self.seat,
                kind: self.kind,
            }
        } else if self.kind == CardKind::Tao {
            EventKind::UseTao {
                seat: self.seat,
                target: self.announced_target.unwrap(),
                card: self.card,
            }
        } else {
            EventKind::UseShan {
                seat: self.seat,
                card: self.card,
            }
        }
    }

    pub(crate) fn use_targets(&self) -> Option<&[SeatId]> {
        Some(self.targets.as_slice())
    }

    pub(crate) fn editable_targets(&mut self) -> Option<&mut Vec<SeatId>> {
        (self.kind == CardKind::Tao && matches!(self.step, Step::Resolve(0)))
            .then_some(&mut self.targets)
    }
}

impl Game {
    pub(crate) fn resolve_basic_use(&mut self, mut flow: Flow, child: Option<Outcome<Resolution>>) {
        match flow.step {
            Step::Pay => {
                if flow.virtual_card || self.classic.is_some() {
                    self.queue_use_costs(flow.seat, &[flow.card]);
                } else {
                    self.move_to_resolving(flow.seat, flow.card.id);
                    self.queue_hero_losses(flow.seat, vec![(Zone::Hand, flow.card)]);
                }
                self.scheduler
                    .after(Program::event(EventKind::Discard { card: flow.card }));
                flow.step = Step::Timing;
                self.scheduler.replace_current(Program::BasicUse(flow));
            }
            Step::Timing => {
                flow.step = Step::Resolve(0);
                self.scheduler
                    .replace_current(Program::BasicUse(flow.clone()));
                if flow.use_card {
                    self.queue_use_card2(flow.seat, flow.kind, &[flow.card]);
                }
            }
            Step::Resolve(index) => {
                if matches!(child, Some(Outcome::Cancelled)) {
                    self.scheduler.end(Outcome::Cancelled);
                } else if flow.kind == CardKind::Shan {
                    self.scheduler.end(Outcome::Finished(Resolution::Dodged));
                } else if index < flow.targets.len() {
                    if index == 0 {
                        self.sort_card_targets(&mut flow.targets);
                    }
                    let target = flow.targets[index];
                    flow.step = Step::Resolve(index + 1);
                    let seat = flow.seat;
                    self.scheduler.replace_current(Program::BasicUse(flow));
                    if self.is_alive(target) {
                        self.next(EventKind::Recover {
                            seat,
                            target,
                            amount: 1 + self.hero_tao_bonus(seat, target),
                        });
                    }
                } else {
                    self.finish();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
