//! WebView and browser bindings for the same authoritative Rust state machine.
use duel_core::{Command, Game, GameSetup, SeatId};
use wasm_bindgen::prelude::*;

mod replay;
pub use replay::RecordedDuel;

#[cfg(feature = "qa-compare")]
mod qa_compare;
#[cfg(feature = "qa-elemental")]
mod qa_elemental;
#[cfg(feature = "qa-mingdian")]
mod qa_mingdian;
#[cfg(feature = "qa-multi-compare")]
mod qa_multi_compare;
#[cfg(feature = "qa-phase-skills")]
mod qa_phase_skills;
#[cfg(feature = "qa-qiaoshui")]
mod qa_qiaoshui;
#[cfg(feature = "qa-recast")]
mod qa_recast;
#[cfg(feature = "qa-rest")]
mod qa_rest;
#[cfg(feature = "qa-judge-area")]
mod qa_judge_area;
#[cfg(feature = "qa-revival")]
mod qa_revival;
#[cfg(feature = "qa-vitality")]
mod qa_vitality;
#[cfg(feature = "qa-xunxun")]
mod qa_xunxun;
#[cfg(feature = "qa-zhongyi")]
mod qa_zhongyi;

#[wasm_bindgen]
pub struct Duel {
    game: Game,
}

#[wasm_bindgen]
impl Duel {
    /// Start a prescribed table, also used to replay cross-runtime conformance scenarios.
    #[wasm_bindgen(constructor)]
    pub fn new(setup: &str) -> Result<Duel, JsError> {
        let parsed: GameSetup = serde_json::from_str(setup)?;
        let game = Game::new(parsed).map_err(|error| JsError::new(&format!("{error:?}")))?;
        Ok(Self { game })
    }

    #[wasm_bindgen(js_name = classic)]
    pub fn classic(seed: u32, seat: SeatId, hero: &str, count: usize) -> Result<Duel, JsError> {
        let game = Game::classic(seed, seat, hero, count)
            .map_err(|error| JsError::new(&format!("{error:?}")))?;
        Ok(Self { game })
    }

    /// Run at most one valid AI decision, leaving presentation pacing to the host.
    pub fn bot(&mut self) -> Result<bool, JsError> {
        let Some(command) = self.game.bot_command() else {
            return Ok(false);
        };
        self.game
            .submit(command)
            .map_err(|error| JsError::new(&format!("{error:?}")))?;
        Ok(true)
    }
    pub fn submit(&mut self, command: &str) -> Result<(), JsError> {
        let parsed: Command = serde_json::from_str(command)?;
        self.game
            .submit(parsed)
            .map_err(|error| JsError::new(&format!("{error:?}")))
    }

    pub fn view(&self, viewer: Option<SeatId>) -> Result<String, JsError> {
        let view = self
            .game
            .view(viewer)
            .map_err(|error| JsError::new(&format!("{error:?}")))?;
        Ok(serde_json::to_string(&view)?)
    }
}
