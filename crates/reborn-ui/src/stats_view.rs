//! The games a player has recorded, for the Stats page: every capture the
//! launcher ingested, kept in the archive beside the queue after the queue
//! is done with it. The page builds the box scores and the summary from
//! this; nothing here decides what is interesting.

use reborn_core::stats::{Capture, PlayerStats, StatsQueue, TeamTotals};
use reborn_core::Platform;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TeamView {
    pub name: String,
    pub score: i64,
}

#[derive(Debug, Serialize)]
pub struct PlayerView {
    pub name: String,
    pub team: String,
    pub side: i64,
    pub stats: PlayerStats,
}

#[derive(Debug, Serialize)]
pub struct GameView {
    pub key: String,
    pub captured_at: String,
    pub source: String,
    pub teams: Vec<TeamView>,
    pub team_totals: Option<Vec<TeamTotals>>,
    pub sides_verified: Option<bool>,
    pub players: Vec<PlayerView>,
}

fn view(key: String, c: Capture) -> GameView {
    GameView {
        key,
        captured_at: c.captured_at,
        source: c.source,
        teams: c.teams.into_iter().map(|t| TeamView { name: t.name, score: t.score }).collect(),
        team_totals: c.team_totals,
        sides_verified: c.sides_verified,
        players: c
            .players
            .into_iter()
            .map(|p| PlayerView { name: p.name, team: p.team, side: p.side, stats: p.stats })
            .collect(),
    }
}

/// Every recorded game, newest first.
#[tauri::command]
pub async fn stats_games() -> Result<Vec<GameView>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let p = Platform::detect();
        let queue = StatsQueue::open(&p).map_err(|e| e.player_message().to_string())?;
        let mut games: Vec<GameView> = queue
            .archived()
            .map_err(|e| e.player_message().to_string())?
            .into_iter()
            .filter_map(|(key, bytes)| Capture::parse(&bytes).ok().map(|c| view(key, c)))
            .collect();
        games.sort_by(|a, b| b.captured_at.cmp(&a.captured_at));
        Ok(games)
    })
    .await
    .map_err(|e| e.to_string())?
}
