use super::skills::{HeroAdjustment, HeroMoment};
use crate::scheduler::{EventRef, Outcome};
use crate::{
    Answer, Card, EventId, EventKind, Game, Phase, Program, Resolution, RuleError, SeatId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhaseInfo {
    pub seat: SeatId,
    pub phase: Phase,
    pub reason: Option<String>,
    pub index: usize,
    pub skipped: bool,
    pub cancelled: bool,
    pub finished: bool,
    /// Fixed discard count set before the `phaseDiscard` request.
    pub discard_required: Option<usize>,
}

#[derive(Default)]
pub(crate) struct State {
    pending: Vec<(SeatId, Phase)>,
    skipped: Vec<(SeatId, Phase)>,
}
impl State {
    pub(crate) fn begin_turn(&mut self) {
        self.skipped.clear();
    }
    pub(crate) fn skipped_play(&self, seat: SeatId) -> bool {
        self.skipped.contains(&(seat, Phase::Play))
    }
    fn record_skip(&mut self, seat: SeatId, phase: Phase) {
        self.skipped.push((seat, phase));
    }
}

#[derive(Clone)]
pub(crate) struct Flow {
    info: PhaseInfo,
    stage: Stage,
    draw: u16,
    judge_card: Option<Card>,
    waiting_hook: bool,
}
#[derive(Clone, Copy)]
enum Stage {
    Enter,
    Before,
    Begin,
    Body,
    Play,
    Judge,
    JudgeCard,
    DrawReady,
    DrawDone,
    DiscardStart,
    Discard,
    End,
    After,
    Finish,
    Skipped,
    Cancelled,
}
impl Flow {
    pub(crate) fn reason(&self) -> Option<&str> {
        self.info.reason.as_deref()
    }
    pub(crate) fn new(seat: SeatId, phase: Phase, reason: Option<String>, index: usize) -> Self {
        Self {
            info: PhaseInfo {
                seat,
                phase,
                reason,
                index,
                skipped: false,
                cancelled: false,
                finished: false,
                discard_required: None,
            },
            stage: Stage::Enter,
            draw: 2,
            judge_card: None,
            waiting_hook: false,
        }
    }
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::Phase {
            seat: self.info.seat,
            phase: self.info.phase,
        }
    }
}

#[derive(Clone, Copy)]
enum Timing {
    Before,
    Begin,
    Body,
    End,
    After,
    Skipped,
    Cancelled,
}
fn timing(phase: Phase, point: Timing) -> &'static str {
    use Timing::*;
    match (phase, point) {
        (Phase::Preparation, Before) => "phaseZhunbeiBefore",
        (Phase::Preparation, Begin) => "phaseZhunbeiBegin",
        (Phase::Preparation, Body) => "phaseZhunbei",
        (Phase::Preparation, End) => "phaseZhunbeiEnd",
        (Phase::Preparation, After) => "phaseZhunbeiAfter",
        (Phase::Preparation, Skipped) => "phaseZhunbeiSkipped",
        (Phase::Preparation, Cancelled) => "phaseZhunbeiCancelled",
        (Phase::Judgment, Before) => "phaseJudgeBefore",
        (Phase::Judgment, Begin) => "phaseJudgeBegin",
        (Phase::Judgment, Body) => "phaseJudge",
        (Phase::Judgment, End) => "phaseJudgeEnd",
        (Phase::Judgment, After) => "phaseJudgeAfter",
        (Phase::Judgment, Skipped) => "phaseJudgeSkipped",
        (Phase::Judgment, Cancelled) => "phaseJudgeCancelled",
        (Phase::Draw, Before) => "phaseDrawBefore",
        (Phase::Draw, Begin) => "phaseDrawBegin",
        (Phase::Draw, Body) => "phaseDraw",
        (Phase::Draw, End) => "phaseDrawEnd",
        (Phase::Draw, After) => "phaseDrawAfter",
        (Phase::Draw, Skipped) => "phaseDrawSkipped",
        (Phase::Draw, Cancelled) => "phaseDrawCancelled",
        (Phase::Play, Before) => "phaseUseBefore",
        (Phase::Play, Begin) => "phaseUseBegin",
        (Phase::Play, Body) => "phaseUse",
        (Phase::Play, End) => "phaseUseEnd",
        (Phase::Play, After) => "phaseUseAfter",
        (Phase::Play, Skipped) => "phaseUseSkipped",
        (Phase::Play, Cancelled) => "phaseUseCancelled",
        (Phase::Discard, Before) => "phaseDiscardBefore",
        (Phase::Discard, Begin) => "phaseDiscardBegin",
        (Phase::Discard, Body) => "phaseDiscard",
        (Phase::Discard, End) => "phaseDiscardEnd",
        (Phase::Discard, After) => "phaseDiscardAfter",
        (Phase::Discard, Skipped) => "phaseDiscardSkipped",
        (Phase::Discard, Cancelled) => "phaseDiscardCancelled",
        (Phase::End, Before) => "phaseJieshuBefore",
        (Phase::End, Begin) => "phaseJieshuBegin",
        (Phase::End, Body) => "phaseJieshu",
        (Phase::End, Timing::End) => "phaseJieshuEnd",
        (Phase::End, After) => "phaseJieshuAfter",
        (Phase::End, Skipped) => "phaseJieshuSkipped",
        (Phase::End, Cancelled) => "phaseJieshuCancelled",
    }
}

impl Game {
    pub fn skip_next_phase(&mut self, seat: SeatId, phase: Phase) -> Result<(), RuleError> {
        if !self.seat(seat)?.alive {
            return Err(RuleError::InactiveSeat { seat });
        }
        if !self.phase_state.pending.contains(&(seat, phase)) {
            self.phase_state.pending.push((seat, phase));
        }
        Ok(())
    }
    pub fn phase_info(&self, event: EventId) -> Option<PhaseInfo> {
        match &self.scheduler.active_frame(event)?.1 {
            Program::Phase(flow) => Some(flow.info.clone()),
            _ => None,
        }
    }
    fn phase_hook(&mut self, flow: &Flow, point: Timing) -> bool {
        let seat = flow.info.seat;
        let name = timing(flow.info.phase, point);
        let native = match (flow.info.phase, point) {
            (Phase::Preparation, Timing::Begin) => Some(HeroMoment::Preparation { seat }),
            (Phase::End, Timing::Begin) => Some(HeroMoment::End { seat }),
            (Phase::Discard, Timing::Before) => Some(HeroMoment::Discard { seat }),
            _ => None,
        };
        if let Some(moment) = native {
            if self.has_hero_triggers(&moment) {
                let dispatch = self.hero_triggers(moment);
                self.scheduler.next(dispatch);
                return true;
            }
        }
        self.queue_timing(name, Some(seat), None, None, 0, &[])
    }

    pub(crate) fn resolve_phase_flow(
        &mut self,
        event: EventRef,
        mut flow: Flow,
        child: Option<Outcome<Resolution>>,
        answer: &mut Option<Answer>,
    ) {
        let seat = flow.info.seat;
        let phase = flow.info.phase;
        if self.match_result().is_some() || !self.seat(seat).expect("the phase seat exists").alive {
            self.finish();
            return;
        }
        if flow.waiting_hook
            && (matches!(child, Some(Outcome::Cancelled))
                || matches!(
                    child,
                    Some(Outcome::Finished(Resolution::Hero(HeroAdjustment::Skip)))
                ))
        {
            flow.info.cancelled = true;
            self.phase_state.record_skip(seat, phase);
            flow.stage = Stage::Cancelled;
        }
        flow.waiting_hook = false;
        loop {
            let hook = match flow.stage {
                Stage::Enter => {
                    self.classic
                        .as_mut()
                        .expect("classic")
                        .turn
                        .as_mut()
                        .expect("turn")
                        .phase = phase;
                    if let Some(index) = self
                        .phase_state
                        .pending
                        .iter()
                        .position(|entry| *entry == (seat, phase))
                    {
                        self.phase_state.pending.remove(index);
                        self.phase_state.record_skip(seat, phase);
                        flow.info.skipped = true;
                        flow.info.finished = true;
                        flow.stage = Stage::Skipped;
                    } else {
                        if phase == Phase::Play {
                            let state = self.classic.as_mut().expect("classic");
                            state.sha_played = false;
                            state.hero_turn.reset_play_phase();
                        }
                        flow.stage = Stage::Before;
                    }
                    continue;
                }
                Stage::Skipped => {
                    flow.stage = Stage::Finish;
                    Some(Timing::Skipped)
                }
                Stage::Cancelled => {
                    flow.stage = Stage::Finish;
                    Some(Timing::Cancelled)
                }
                Stage::Before => {
                    flow.stage = Stage::Begin;
                    Some(Timing::Before)
                }
                Stage::Begin => {
                    flow.stage = Stage::Body;
                    Some(Timing::Begin)
                }
                Stage::Body => match phase {
                    Phase::Preparation | Phase::End => {
                        flow.stage = Stage::End;
                        Some(Timing::Body)
                    }
                    Phase::Judgment => {
                        flow.stage = Stage::Judge;
                        continue;
                    }
                    Phase::Draw => {
                        flow.stage = Stage::DrawReady;
                        let moment = HeroMoment::Draw {
                            seat,
                            amount: flow.draw,
                        };
                        if self.wants_timing("phaseDrawBegin1")
                            || self.wants_timing("phaseDrawBegin2")
                            || self.has_hero_triggers(&moment)
                        {
                            let dispatch = self.hero_triggers(moment);
                            flow.waiting_hook = true;
                            self.scheduler.replace_current(Program::Phase(flow));
                            self.scheduler.next(dispatch);
                            return;
                        }
                        continue;
                    }
                    Phase::Play => {
                        flow.stage = Stage::Play;
                        continue;
                    }
                    Phase::Discard => {
                        flow.stage = Stage::DiscardStart;
                        continue;
                    }
                },
                Stage::Play => {
                    let done = self.resolve_phase_play(seat, answer);
                    if done {
                        flow.stage = Stage::End;
                    }
                    self.scheduler.replace_current(Program::Phase(flow));
                    return;
                }
                Stage::Judge => {
                    if let Some(card) = self.seat(seat).expect("seat").judges.last().copied() {
                        flow.judge_card = Some(card);
                        flow.stage = Stage::JudgeCard;
                        self.scheduler.replace_current(Program::Phase(flow.clone()));
                        let queued = self.phase_hook(&flow, Timing::Body);
                        flow.waiting_hook = queued;
                        self.scheduler.replace_current(Program::Phase(flow.clone()));
                        if queued {
                            return;
                        }
                        continue;
                    }
                    flow.stage = Stage::End;
                    continue;
                }
                Stage::JudgeCard => {
                    let card = flow.judge_card.take().expect("judgment material selected");
                    flow.stage = Stage::Judge;
                    if self.seat(seat).is_ok_and(|p| p.judges.contains(&card)) {
                        self.scheduler.next(self.trick_judgment(seat, card));
                        self.scheduler.replace_current(Program::Phase(flow));
                        return;
                    }
                    continue;
                }
                Stage::DrawReady => {
                    if let Some(Outcome::Finished(Resolution::Hero(HeroAdjustment::Draw(amount)))) =
                        child
                    {
                        flow.draw = amount;
                    }
                    flow.stage = Stage::DrawDone;
                    if flow.draw > 0 {
                        self.queue_draw(seat, flow.draw, crate::DrawReason::Phase);
                        self.scheduler.replace_current(Program::Phase(flow));
                        return;
                    }
                    continue;
                }
                Stage::DrawDone => {
                    flow.stage = Stage::End;
                    continue;
                }
                Stage::DiscardStart => {
                    let player = self.seat(seat).expect("seat");
                    let required = player
                        .hand
                        .len()
                        .saturating_sub(usize::try_from(player.hp).unwrap_or_default());
                    flow.info.discard_required = Some(required);
                    if required == 0 {
                        flow.stage = Stage::End;
                        continue;
                    }
                    flow.stage = Stage::Discard;
                    Some(Timing::Body)
                }
                Stage::Discard => {
                    let done = self.resolve_discard_phase(
                        event,
                        seat,
                        flow.info.discard_required.expect("discard count fixed"),
                        answer,
                    );
                    if done {
                        flow.stage = Stage::End;
                    }
                    self.scheduler.replace_current(Program::Phase(flow));
                    return;
                }
                Stage::End => {
                    flow.info.finished = phase != Phase::Play;
                    flow.stage = Stage::After;
                    Some(Timing::End)
                }
                Stage::After => {
                    flow.stage = Stage::Finish;
                    Some(Timing::After)
                }
                Stage::Finish => {
                    self.finish();
                    return;
                }
            };
            if let Some(point) = hook {
                // Request observers see cancelled before the event finishes; callbacks see finished.
                self.scheduler.replace_current(Program::Phase(flow.clone()));
                let queued = self.phase_hook(&flow, point);
                flow.waiting_hook = queued && !matches!(point, Timing::Skipped | Timing::Cancelled);
                if matches!(point, Timing::Cancelled) {
                    flow.info.finished = true;
                }
                self.scheduler.replace_current(Program::Phase(flow.clone()));
                if queued {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "phases_tests.rs"]
mod tests;
