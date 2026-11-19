//! Rest keeps a character in the living roster while removing them from play.
use crate::scheduler::Outcome;
use crate::{
    EventId, EventKind, Game, Program, Resolution, SeatId, TriggerId, TriggerOrder,
    TriggerRegistration, TriggerRole, TriggerStep, TurnInfo,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[cfg(test)]
mod natural_tests;
#[cfg(test)]
mod participation_tests;
#[cfg(test)]
mod stale_clock_tests;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RestUnit {
    Phase,
    Round,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestClock {
    pub unit: RestUnit,
    /// Counts at or below zero stay in effect until explicitly ended.
    pub count: i32,
}

impl Default for RestClock {
    fn default() -> Self {
        Self {
            unit: RestUnit::Phase,
            count: -1,
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Enter(RestClock),
    End(i32),
}

#[derive(Clone, Copy)]
enum Stage {
    Apply,
    Notify,
    Notified,
    Cancelled,
}

#[derive(Clone)]
pub(crate) struct Flow {
    seat: SeatId,
    kind: Kind,
    stage: Stage,
}

impl Flow {
    pub(crate) fn rest(seat: SeatId, clock: RestClock) -> Self {
        Self {
            seat,
            kind: Kind::Enter(clock),
            stage: Stage::Apply,
        }
    }

    pub(crate) fn end(seat: SeatId, hp: i32) -> Self {
        Self {
            seat,
            kind: Kind::End(hp),
            stage: Stage::Apply,
        }
    }

    pub(crate) fn kind(&self) -> EventKind {
        let seat = self.seat;
        match self.kind {
            Kind::Enter(clock) => EventKind::Rest { seat, clock },
            Kind::End(hp) => EventKind::RestEnd { seat, hp },
        }
    }

    fn timing(&self) -> &'static str {
        match (self.kind, self.stage) {
            (Kind::Enter(_), Stage::Cancelled) => "restCancelled",
            (Kind::End(_), Stage::Cancelled) => "restEndCancelled",
            (Kind::Enter(_), _) => "rest",
            (Kind::End(_), _) => "restEnd",
        }
    }
}

impl Game {
    /// Whether the seat is currently out of play.
    pub fn is_out(&self, seat: SeatId) -> bool {
        self.seat(seat).is_ok_and(|player| player.out)
    }

    /// Whether the seat is alive and participating in play.
    pub fn is_in(&self, seat: SeatId) -> bool {
        self.seat(seat)
            .is_ok_and(|player| player.alive && !player.out)
    }

    pub fn rest_clock(&self, seat: SeatId) -> Option<RestClock> {
        self.seat(seat).ok()?.rest_clock
    }

    /// Whether the rest mark is visible, independently of the saved clock.
    pub fn has_rest_mark(&self, seat: SeatId) -> bool {
        self.seat(seat).is_ok_and(|player| player.rest_mark_shown)
    }

    pub fn is_resting(&self, seat: SeatId) -> bool {
        self.is_alive(seat) && self.is_out(seat) && self.rest_clock(seat).is_some()
    }

    /// Find the next living seat that can be reached through the seating ring.
    pub fn next_participant(&self, seat: SeatId) -> Result<Option<SeatId>, crate::RuleError> {
        let order = self.seat_order_from(seat)?;
        if self.has_skill(seat, "undist") {
            return Ok(None);
        }
        Ok(order
            .into_iter()
            .skip(1)
            .find(|&next| self.is_alive(next) && !self.has_skill(next, "undist")))
    }

    pub fn rest_cancelled(&self, event: EventId) -> Option<bool> {
        match self.scheduler.active_frame(event)? {
            (_, Program::Rest(flow)) => Some(matches!(flow.stage, Stage::Cancelled)),
            _ => None,
        }
    }

    pub(crate) fn in_order_from(&self, start: SeatId) -> Vec<SeatId> {
        self.seat_order_from(start)
            .expect("the starting seat is present")
            .into_iter()
            .filter(|&seat| self.is_in(seat))
            .collect()
    }

    fn install_rest_clock(&mut self, seat: SeatId) {
        let trigger = self
            .register_trigger(TriggerRegistration {
                owner: seat,
                skill: "_rest_return".into(),
                timing: "phaseBefore".into(),
                role: TriggerRole::Global,
                order: TriggerOrder::Normal,
                priority: 0,
                label: None,
                silent: true,
                filter: Arc::new(|game, event, owner| {
                    game.can_tick_rest_clock(owner, event.subject)
                }),
                effect: Arc::new(|ctx, _, _| {
                    ctx.tick_rest_clock();
                    TriggerStep::Done
                }),
            })
            .expect("rest clocks register on existing seats");
        self.set_trigger_force_die(trigger, true)
            .expect("the rest clock trigger was registered");
        self.set_trigger_force_out(trigger, true)
            .expect("the rest clock trigger was registered");
        self.seat_mut(seat).rest_trigger = Some(trigger);
    }

    fn qualifying_rest_turn(&self, seat: SeatId, subject: EventId) -> Option<TurnInfo> {
        let clock = self.rest_clock(seat).filter(|clock| clock.count > 0)?;
        self.turn_info(subject).filter(|turn| {
            clock.unit == RestUnit::Phase || (turn.seat == seat && turn.reason.is_none())
        })
    }

    fn can_tick_rest_clock(&self, seat: SeatId, subject: EventId) -> bool {
        self.is_out(seat)
            && self
                .qualifying_rest_turn(seat, subject)
                .is_some_and(|turn| !turn.rest_counted)
    }

    /// The clock's phaseBefore predicate drops stale storage for a player already back in play.
    /// This does not run a skill effect, spend the turn's clock tick, or remove the mark or skill.
    pub(crate) fn clear_stale_rest_clock(&mut self, id: TriggerId, subject: EventId) {
        let Some(seat) = self
            .trigger_registrations
            .get(&id)
            .map(|registration| registration.owner)
        else {
            return;
        };
        if self
            .seat(seat)
            .is_ok_and(|player| player.is_in() && player.rest_trigger == Some(id))
            && self.qualifying_rest_turn(seat, subject).is_some()
        {
            let player = self.seat_mut(seat);
            player.rest_clock = None;
            player.rest_trigger = None;
            self.unregister_trigger(id);
        }
    }

    /// Decrement before the turn continues. Only the first eligible callback per turn can count.
    pub(crate) fn tick_rest_clock(&mut self, seat: SeatId, subject: EventId) -> bool {
        if !self.can_tick_rest_clock(seat, subject) || !self.count_turn_rest(subject) {
            return false;
        }
        let clock = self
            .seat_mut(seat)
            .rest_clock
            .as_mut()
            .expect("the active clock was validated");
        clock.count -= 1;
        clock.count == 0
    }

    pub(crate) fn resolve_rest(&mut self, mut flow: Flow, child: Option<Outcome<Resolution>>) {
        let seat = flow.seat;
        if matches!(flow.stage, Stage::Notified) && matches!(child, Some(Outcome::Cancelled)) {
            flow.stage = Stage::Cancelled;
            let timing = flow.timing();
            self.scheduler.replace_current(Program::Rest(flow.clone()));
            if self.queue_timing(timing, Some(seat), None, None, 0, &[]) {
                return;
            }
        }

        loop {
            match flow.stage {
                Stage::Apply => {
                    flow.stage = Stage::Notify;
                    match flow.kind {
                        Kind::Enter(clock) => {
                            if self.rest_clock(seat).is_some() {
                                self.finish();
                                return;
                            }
                            self.set_seat_alive(seat, true);
                            let seats = &self.seats;
                            self.living_seats.sort_by_key(|id| {
                                seats
                                    .iter()
                                    .position(|player| player.id == *id)
                                    .expect("the living seat is present")
                            });
                            let player = self.seat_mut(seat);
                            player.rest_clock = Some(clock);
                            player.rest_mark_shown = true;
                            player.rest_undist = true;
                            player.out = true;
                            if clock.count > 0 {
                                self.install_rest_clock(seat);
                            }
                        }
                        Kind::End(hp) => {
                            let player = self.seat_mut(seat);
                            if player.rest_clock.take().is_none() {
                                self.finish();
                                return;
                            }
                            player.out = false;
                            player.rest_mark_shown = false;
                            player.rest_undist = false;
                            if let Some(trigger) = player.rest_trigger.take() {
                                self.unregister_trigger(trigger);
                            }
                            self.scheduler.replace_current(Program::Rest(flow));
                            self.scheduler.next(self.revive_program(seat, Some(hp)));
                            return;
                        }
                    }
                }
                Stage::Notify => {
                    flow.stage = Stage::Notified;
                    let timing = flow.timing();
                    self.scheduler.replace_current(Program::Rest(flow.clone()));
                    if self.queue_timing(timing, Some(seat), None, None, 0, &[]) {
                        return;
                    }
                }
                Stage::Notified => {
                    self.finish();
                    return;
                }
                Stage::Cancelled => {
                    self.scheduler
                        .end(Outcome::Finished(Resolution::RestCancelled));
                    return;
                }
            }
        }
    }
}
