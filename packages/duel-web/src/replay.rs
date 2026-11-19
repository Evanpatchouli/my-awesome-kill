//! Replay recording is opt-in and separate from a seat's normal view.

use crate::Duel;
use duel_core::content::ContentPack;
use duel_core::{Command, RecordedGame, ReplayLog, ReplaySetup, SeatId};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct RecordedDuel {
    session: RecordedGame,
}

#[wasm_bindgen]
impl RecordedDuel {
    #[wasm_bindgen(js_name = classicWithContent)]
    pub fn classic_with_content(
        seed: u32,
        human: SeatId,
        hero: &str,
        seats: usize,
        raw: &str,
    ) -> Result<RecordedDuel, JsError> {
        let pack = ContentPack::parse(raw).map_err(|error| JsError::new(&error.to_string()))?;
        let setup = ReplaySetup::SeededContent {
            seed,
            human,
            hero: hero.to_owned(),
            seats,
            pack,
        };
        let session = RecordedGame::new(setup).map_err(|error| JsError::new(&error.to_string()))?;
        Ok(Self { session })
    }

    #[wasm_bindgen(constructor)]
    pub fn new(setup: &str) -> Result<RecordedDuel, JsError> {
        let setup: ReplaySetup = serde_json::from_str(setup)?;
        let session = RecordedGame::new(setup).map_err(|error| JsError::new(&error.to_string()))?;
        Ok(Self { session })
    }

    pub fn submit(&mut self, command: &str) -> Result<(), JsError> {
        let command: Command = serde_json::from_str(command)?;
        self.session
            .submit(command)
            .map_err(|error| JsError::new(&error.to_string()))
    }

    pub fn bot(&mut self) -> Result<bool, JsError> {
        self.session
            .bot()
            .map_err(|error| JsError::new(&error.to_string()))
    }

    pub fn view(&self, seat: Option<SeatId>) -> Result<String, JsError> {
        let view = self
            .session
            .game()
            .view(seat)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(serde_json::to_string(&view)?)
    }

    /// Obtain the full private record as the host of this table.
    pub fn log(&self) -> Result<String, JsError> {
        Ok(serde_json::to_string(self.session.log())?)
    }

    #[wasm_bindgen(js_name = replayAt)]
    pub fn replay_at(log: &str, accepted: usize) -> Result<Duel, JsError> {
        let log: ReplayLog = serde_json::from_str(log)?;
        let game = log
            .restore_at(accepted)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(Duel { game })
    }
}
