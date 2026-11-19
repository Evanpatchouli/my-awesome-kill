//! Persistent natural turn loop, extra-turn queue, and per-turn phase plans.
use super::phases;
use super::skills::HeroTurn;
use crate::scheduler::Outcome;
use crate::{
    EventId, EventKind, Game, Phase, Program, Resolution, RuleError, SeatId, TriggerEvent, TurnView,
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, LazyLock};

#[cfg(test)]
#[path = "turns_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhasePlan {
    pub phase: Phase,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnInfo {
    pub event: EventId,
    pub seat: SeatId,
    pub number: u64,
    pub round: u64,
    pub reason: Option<String>,
    pub skipped: bool,
    pub cancelled: bool,
    pub personal_started: bool,
    pub plan: Arc<[PhasePlan]>,
    pub next: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnPosition {
    Back,
    Front,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueueStatus {
    Pending,
    Running,
    Finished,
}

#[derive(Clone, Debug)]
pub struct QueuedTurn {
    pub seat: SeatId,
    pub reason: Option<String>,
    status: QueueStatus,
}

#[derive(Default)]
pub(crate) struct State {
    loop_id: Option<EventId>,
    current: Option<SeatId>,
    queue: VecDeque<QueuedTurn>,
    history: Vec<TurnInfo>,
    personal: BTreeMap<SeatId, u64>,
}
#[derive(Clone, Copy, Debug)]
enum TurnStage {
    Before,
    Count,
    RoundStart,
    BeforeStart,
    BeforeEnd,
    Turned,
    BeginStart,
    Begin,
    Change,
    RunPhase,
    AfterPhase,
    End,
    After,
    Finish,
    FlipDispatch,
    FlipFinish,
}
#[derive(Clone)]
pub(crate) struct TurnFlow {
    info: TurnInfo,
    stage: TurnStage,
    round_start: bool,
}
impl TurnFlow {
    pub(crate) fn reason(&self) -> Option<&str> {
        self.info.reason.as_deref()
    }
    pub(crate) fn new(
        seat: SeatId,
        number: u64,
        round: u64,
        round_start: bool,
        reason: Option<String>,
    ) -> Self {
        static NORMAL_PLAN: LazyLock<Arc<[PhasePlan]>> = LazyLock::new(|| {
            [
                Phase::Preparation,
                Phase::Judgment,
                Phase::Draw,
                Phase::Play,
                Phase::Discard,
                Phase::End,
            ]
            .into_iter()
            .map(|phase| PhasePlan {
                phase,
                reason: None,
            })
            .collect()
        });
        Self {
            info: TurnInfo {
                event: 0,
                seat,
                number,
                round,
                reason,
                skipped: false,
                cancelled: false,
                personal_started: false,
                plan: Arc::clone(&NORMAL_PLAN),
                next: 0,
            },
            stage: TurnStage::Before,
            round_start,
        }
    }
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::Turn {
            seat: self.info.seat,
            number: self.info.number,
            round: self.info.round,
        }
    }
}
#[derive(Clone, Copy)]
enum LoopStage {
    Init,
    BeginNatural,
    DrainExtra,
    Over,
    Advance,
    RoundEnd,
    FinishRound,
}
#[derive(Clone)]
pub(crate) struct LoopFlow {
    stage: LoopStage,
    natural: Option<SeatId>,
    first: bool,
    new_round: bool,
    awaiting_turn: bool,
}
impl LoopFlow {
    pub(crate) fn new() -> Self {
        Self {
            stage: LoopStage::Init,
            natural: None,
            first: true,
            new_round: false,
            awaiting_turn: false,
        }
    }
    pub(crate) fn kind(&self) -> EventKind {
        EventKind::PhaseLoop
    }
}
impl Game {
    pub fn turn_info(&self, subject: EventId) -> Option<TurnInfo> {
        match &self.scheduler.active_frame(subject)?.1 {
            Program::Turn(flow) => {
                let mut info = flow.info.clone();
                info.event = subject;
                Some(info)
            }
            _ => None,
        }
    }
    pub fn turn_history(&self) -> &[TurnInfo] {
        &self.turns.history
    }
    pub fn current_phase_player(&self) -> Option<SeatId> {
        self.turns.current
    }
    /// The natural seat being processed by the live phase loop, including during extra turns.
    pub fn natural_phase_player(&self) -> Option<SeatId> {
        let id = self.turns.loop_id?;
        match self.scheduler.active_frame(id)?.1 {
            Program::PhaseLoop(flow) => flow.natural,
            _ => None,
        }
    }
    pub fn turn_number(&self) -> u64 {
        self.classic
            .as_ref()
            .and_then(|s| s.turn)
            .map_or(0, |turn| turn.number)
    }
    pub fn round_number(&self) -> u64 {
        self.classic
            .as_ref()
            .and_then(|s| s.turn)
            .map_or(0, |turn| turn.round)
    }
    pub fn personal_turn_number(&self, seat: SeatId) -> u64 {
        self.turns.personal.get(&seat).copied().unwrap_or(0)
    }
    pub fn insert_next_phase(
        &mut self,
        event: &TriggerEvent,
        phase: Phase,
        reason: &str,
    ) -> Result<(), RuleError> {
        if !matches!(event.timing.as_str(), "phaseBegin" | "phaseChange") {
            return Err(RuleError::WrongEvent);
        }
        let Some(Program::Turn(flow)) = self.scheduler.active_program_mut(event.subject) else {
            return Err(RuleError::WrongEvent);
        };
        let valid = match event.timing.as_str() {
            "phaseBegin" => matches!(flow.stage, TurnStage::Change),
            "phaseChange" => matches!(flow.stage, TurnStage::RunPhase),
            _ => false,
        };
        if !valid {
            return Err(RuleError::WrongEvent);
        }
        let mut plan = flow.info.plan.to_vec();
        plan.insert(
            flow.info.next,
            PhasePlan {
                phase,
                reason: Some(reason.to_owned()),
            },
        );
        flow.info.plan = plan.into();
        let updated_plan = Arc::clone(&flow.info.plan);
        if let Some(row) = self
            .turns
            .history
            .last_mut()
            .filter(|row| row.event == event.subject)
        {
            row.plan = updated_plan;
        }
        Ok(())
    }
    pub fn insert_extra_turn(&mut self, seat: SeatId, reason: &str) -> Result<(), RuleError> {
        self.insert_extra_turn_at(seat, reason, TurnPosition::Back)
    }

    /// The loop's ordered queue includes the event it is currently awaiting.
    pub fn queued_turns(&self) -> impl Iterator<Item = &QueuedTurn> {
        self.turns.queue.iter()
    }

    pub fn insert_extra_turn_at(
        &mut self,
        seat: SeatId,
        reason: &str,
        position: TurnPosition,
    ) -> Result<(), RuleError> {
        self.seat(seat)?;
        let id = self.turns.loop_id.ok_or(RuleError::WrongEvent)?;
        if !matches!(
            self.scheduler.active_frame(id),
            Some((_, Program::PhaseLoop(_)))
        ) {
            return Err(RuleError::WrongEvent);
        }
        let turn = QueuedTurn {
            seat,
            reason: Some(reason.to_owned()),
            status: QueueStatus::Pending,
        };
        match position {
            TurnPosition::Back => self.turns.queue.push_back(turn),
            TurnPosition::Front => self.turns.queue.push_front(turn),
        }
        Ok(())
    }
    fn update_turn_history(&mut self, info: &TurnInfo) {
        if let Some(row) = self
            .turns
            .history
            .last_mut()
            .filter(|row| row.event == info.event)
        {
            row.next = info.next;
            row.skipped = info.skipped;
            row.cancelled = info.cancelled;
            row.personal_started = info.personal_started;
        }
    }
    pub(crate) fn start_next_turn(&mut self) -> bool {
        let Some(state) = self.classic.as_ref() else {
            return false;
        };
        if let Some(result) = state.result {
            if state.result_announced {
                return false;
            }
            self.classic.as_mut().unwrap().result_announced = true;
            self.scheduler.start(Program::event(EventKind::GameOver {
                winner: result.winner(),
            }));
        } else {
            self.scheduler.start(Program::PhaseLoop(LoopFlow::new()));
        }
        true
    }
    pub(crate) fn resolve_phase_loop(
        &mut self,
        mut flow: LoopFlow,
        _child: Option<Outcome<Resolution>>,
    ) {
        if self.match_result().is_some() {
            self.turns.current = None;
            self.turns.loop_id = None;
            self.turns.queue.clear();
            self.finish();
            return;
        }
        self.turns.loop_id = Some(self.scheduler.current_id());
        if flow.awaiting_turn {
            // waitNext completes the awaited child and then removes the current head. A
            // front insertion during that wait changes which element the loop removes.
            if let Some(awaited) = self
                .turns
                .queue
                .iter_mut()
                .find(|turn| turn.status == QueueStatus::Running)
            {
                awaited.status = QueueStatus::Finished;
            }
            self.turns.queue.pop_front();
            flow.awaiting_turn = false;
        }
        loop {
            match flow.stage {
                LoopStage::Init => {
                    let state = self.classic.as_ref().expect("classic loop");
                    let order = self.seat_order_from(state.lord).unwrap();
                    flow.first = state.turn.is_none();
                    flow.natural = Some(if let Some(turn) = state.turn {
                        let index = order.iter().position(|&id| id == turn.seat).unwrap();
                        order[(index + 1) % order.len()]
                    } else {
                        state.lord
                    });
                    flow.new_round = !flow.first && flow.natural == Some(state.lord);
                    flow.stage = if flow.new_round {
                        LoopStage::RoundEnd
                    } else {
                        LoopStage::BeginNatural
                    };
                }
                LoopStage::RoundEnd => {
                    flow.stage = LoopStage::FinishRound;
                    self.scheduler
                        .replace_current(Program::PhaseLoop(flow.clone()));
                    if self.queue_timing("roundEnd", flow.natural, None, None, 0, &[]) {
                        return;
                    }
                }
                LoopStage::FinishRound => {
                    flow.stage = LoopStage::BeginNatural;
                }
                LoopStage::BeginNatural => {
                    let seat = flow.natural.expect("natural loop seat");
                    if self.is_alive(seat) {
                        self.turns.queue.push_back(QueuedTurn {
                            seat,
                            reason: None,
                            status: QueueStatus::Pending,
                        });
                    }
                    flow.stage = LoopStage::DrainExtra;
                }
                LoopStage::DrainExtra => {
                    while self
                        .turns
                        .queue
                        .front()
                        .is_some_and(|turn| turn.status == QueueStatus::Finished)
                    {
                        self.turns.queue.pop_front();
                    }
                    if let Some(next) = self.turns.queue.front_mut() {
                        next.status = QueueStatus::Running;
                        let seat = next.seat;
                        let reason = next.reason.clone();
                        let starts_round = reason.is_none() && (flow.first || flow.new_round);
                        let turn = TurnFlow::new(
                            seat,
                            self.turn_number() + 1,
                            self.round_number() + u64::from(starts_round),
                            starts_round,
                            reason,
                        );
                        flow.awaiting_turn = true;
                        self.scheduler.replace_current(Program::PhaseLoop(flow));
                        self.scheduler.next(Program::Turn(turn));
                        return;
                    }
                    flow.stage = LoopStage::Over;
                }
                LoopStage::Over => {
                    self.turns.current = None;
                    flow.stage = LoopStage::Advance;
                    self.scheduler
                        .replace_current(Program::PhaseLoop(flow.clone()));
                    if self.queue_timing("phaseOver", flow.natural, None, None, 0, &[]) {
                        return;
                    }
                }
                LoopStage::Advance => {
                    let lord = self.classic.as_ref().unwrap().lord;
                    let order = self.seat_order_from(lord).unwrap();
                    let index = order
                        .iter()
                        .position(|&seat| Some(seat) == flow.natural)
                        .unwrap();
                    flow.natural = Some(order[(index + 1) % order.len()]);
                    flow.first = false;
                    flow.new_round = flow.natural == Some(lord);
                    flow.stage = if flow.new_round {
                        LoopStage::RoundEnd
                    } else {
                        LoopStage::BeginNatural
                    };
                }
            }
        }
    }
    pub(crate) fn resolve_turn_flow(
        &mut self,
        mut flow: TurnFlow,
        child: Option<Outcome<Resolution>>,
    ) {
        if flow.info.event == 0 {
            flow.info.event = self.scheduler.current_id();
        }
        if self.match_result().is_some() {
            self.turns.current = None;
            self.finish();
            return;
        }
        if matches!(child, Some(Outcome::Cancelled))
            && !matches!(
                flow.stage,
                TurnStage::AfterPhase | TurnStage::FlipDispatch | TurnStage::FlipFinish
            )
        {
            flow.info.cancelled = true;
            self.update_turn_history(&flow.info);
            flow.stage = TurnStage::FlipFinish;
            let seat = flow.info.seat;
            self.scheduler.replace_current(Program::Turn(flow));
            self.queue_timing("phaseCancelled", Some(seat), None, None, 0, &[]);
            return;
        }
        let seat = flow.info.seat;
        loop {
            let hook = match flow.stage {
                TurnStage::Before => {
                    flow.stage = TurnStage::Count;
                    Some("phaseBefore")
                }
                TurnStage::Count => {
                    for player in &mut self.seats {
                        player.turn_kills = 0;
                    }
                    for player in &mut self.seats {
                        player.turn_kills = 0;
                    }
                    let state = self.classic.as_mut().expect("classic turn");
                    state.turn = Some(TurnView {
                        seat,
                        number: flow.info.number,
                        round: flow.info.round,
                        phase: Phase::Preparation,
                    });
                    state.sha_played = false;
                    state.hero_turn = HeroTurn::default();
                    self.active_seat = seat;
                    self.phase_state.begin_turn();
                    self.turns.history.push(flow.info.clone());
                    flow.stage = TurnStage::RoundStart;
                    continue;
                }
                TurnStage::RoundStart => {
                    flow.stage = TurnStage::BeforeStart;
                    flow.round_start.then_some("roundStart")
                }
                TurnStage::BeforeStart => {
                    flow.stage = TurnStage::BeforeEnd;
                    Some("phaseBeforeStart")
                }
                TurnStage::BeforeEnd => {
                    flow.stage = TurnStage::Turned;
                    Some("phaseBeforeEnd")
                }
                TurnStage::Turned => {
                    if self.is_alive(seat) && self.seat(seat).unwrap().turned_over {
                        flow.info.skipped = true;
                        flow.info.cancelled = true;
                        self.update_turn_history(&flow.info);
                        flow.stage = TurnStage::FlipDispatch;
                        self.scheduler.replace_current(Program::Turn(flow.clone()));
                        let dispatch =
                            self.prepare_timing("phaseCancelled", Some(seat), None, None, 0, &[]);
                        if let Some(flip) =
                            self.seat_change(seat, super::elemental::Change::TurnOver(false))
                        {
                            self.scheduler.next(flip);
                        }
                        if let Some(event) = dispatch {
                            self.scheduler.next(Program::Dispatch(event));
                        }
                        return;
                    }
                    self.turns.current = Some(seat);
                    *self.turns.personal.entry(seat).or_default() += 1;
                    flow.info.personal_started = true;
                    self.update_turn_history(&flow.info);
                    flow.stage = TurnStage::BeginStart;
                    continue;
                }
                TurnStage::FlipDispatch => {
                    flow.stage = TurnStage::FlipFinish;
                    continue;
                }
                TurnStage::FlipFinish => {
                    self.turns.current = None;
                    self.scheduler.end(Outcome::Cancelled);
                    return;
                }
                TurnStage::BeginStart => {
                    flow.stage = TurnStage::Begin;
                    Some("phaseBeginStart")
                }
                TurnStage::Begin => {
                    flow.stage = TurnStage::Change;
                    Some("phaseBegin")
                }
                TurnStage::Change => {
                    if !self.is_alive(seat) || flow.info.next >= flow.info.plan.len() {
                        flow.stage = TurnStage::End;
                        continue;
                    }
                    flow.stage = TurnStage::RunPhase;
                    Some("phaseChange")
                }
                TurnStage::RunPhase => {
                    let index = flow.info.next;
                    let Some(next) = flow.info.plan.get(index).cloned() else {
                        flow.stage = TurnStage::End;
                        continue;
                    };
                    flow.info.next += 1;
                    self.update_turn_history(&flow.info);
                    flow.stage = TurnStage::AfterPhase;
                    self.scheduler.replace_current(Program::Turn(flow));
                    self.scheduler.next(Program::Phase(phases::Flow::new(
                        seat,
                        next.phase,
                        next.reason,
                        index,
                    )));
                    return;
                }
                TurnStage::AfterPhase => {
                    flow.stage = TurnStage::Change;
                    continue;
                }
                TurnStage::End => {
                    flow.stage = TurnStage::After;
                    Some("phaseEnd")
                }
                TurnStage::After => {
                    flow.stage = TurnStage::Finish;
                    Some("phaseAfter")
                }
                TurnStage::Finish => {
                    self.turns.current = None;
                    self.update_turn_history(&flow.info);
                    self.finish();
                    return;
                }
            };
            if let Some(timing) = hook {
                self.scheduler.replace_current(Program::Turn(flow.clone()));
                if self.queue_timing(timing, Some(seat), None, None, 0, &[]) {
                    return;
                }
            }
        }
    }
}
