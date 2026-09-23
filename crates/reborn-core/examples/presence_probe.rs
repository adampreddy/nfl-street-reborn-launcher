//! Show a real presence in a real Discord, because a passing unit test proves only that the copy
//! is right - not that anything appears on anyone's profile.
//!
//! Run:  cargo run -p reborn-core --example presence_probe
//!
//! Cycles through several states with a pause between each, so you can watch your own Discord
//! profile change. Clears on the way out. If Discord is not running this prints that plainly and
//! exits 0, because Discord not running is the normal case and never an error anywhere else either.

use std::thread::sleep;
use std::time::Duration;

use reborn_core::presence::{payload, Activity, Mode, Presence, Series};
use reborn_core::presence_discord::{now_unix, DiscordPresence, DEFAULT_APP_ID};

fn main() {
    let hold = Duration::from_secs(
        std::env::var("PRESENCE_HOLD_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(6),
    );
    let started = now_unix();

    let states: Vec<(&str, Activity)> = vec![
        ("launcher", Activity::Launcher),
        ("browsing a page", Activity::Browsing { page: "Mods" }),
        ("offline game (must say nothing else)", Activity::Offline),
        ("in the game's menus", Activity::Menus),
        ("ranked queue", Activity::Queued { mode: Mode::Ranked, start_unix: started }),
        (
            "lobby with a join button",
            Activity::Lobby { mode: Mode::Casual, code: "K7QP2M".into(), players: 1, capacity: 2 },
        ),
        (
            "starting a match",
            Activity::Preparing { mode: Mode::Ranked, opponent: "Jock".into(), field: Some("Brooklyn".into()) },
        ),
        (
            "live match, no score yet",
            Activity::InMatch {
                mode: Mode::Ranked,
                opponent: "Jock".into(),
                field: Some("Brooklyn".into()),
                series: Some(Series { game: 1, best_of: 3, wins_self: 0, wins_opponent: 0 }),
                score: None,
                start_unix: started,
            },
        ),
        (
            "live match WITH score",
            Activity::InMatch {
                mode: Mode::Ranked,
                opponent: "Jock".into(),
                field: Some("Brooklyn".into()),
                series: Some(Series { game: 2, best_of: 3, wins_self: 1, wins_opponent: 0 }),
                score: Some((28, 21)),
                start_unix: started,
            },
        ),
    ];

    println!("app id {DEFAULT_APP_ID}, {} states, {}s each", states.len(), hold.as_secs());
    println!("watch your own Discord profile.\n");

    let mut presence = DiscordPresence::with_default_app();
    let mut shown = 0usize;

    for (label, activity) in &states {
        let body = payload(activity);
        match presence.set(&body) {
            Ok(()) => {
                shown += 1;
                println!("  OK   {label}");
                println!("        details: {}", body.details);
                if !body.state.is_empty() {
                    println!("        state  : {}", body.state);
                }
                println!("        art    : {} ({})", body.large_image, body.large_text);
                if let Some(s) = &body.join_secret {
                    println!("        join   : {s}");
                }
            }
            Err(e) => {
                println!("  FAIL {label}: {e}");
                if shown == 0 {
                    println!("\nNothing was shown. Discord is probably not running, which is not an error:");
                    println!("the launcher logs this and carries on. Start Discord and run again to see it.");
                    return;
                }
            }
        }
        sleep(hold);
    }

    let _ = presence.clear();
    println!("\n{shown}/{} states shown, presence cleared.", states.len());
}
