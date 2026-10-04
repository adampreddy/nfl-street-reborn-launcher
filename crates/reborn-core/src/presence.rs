//! What Discord shows about a player, and the rules about what it must never show.
//!
//! Two halves, deliberately split. THIS module is pure: an [`Activity`] in, a [`Payload`] out, no
//! IO and no Discord types, so every line of copy and every privacy rule is unit-testable without a
//! running Discord. The transport that actually talks to Discord's local IPC socket lives behind
//! [`Presence`] and is the only part that can fail.
//!
//! WHO PUBLISHES. The launcher process, and only the launcher process. It does not exit while a
//! game runs: `dolphin_settings::netplay_open` keeps a thread polling Dolphin's status file every
//! 750 ms and holds the launch guard until the child exits, and `Session::run` likewise waits on
//! the child. So one publisher covers menus, queue, lobby and the whole match.
//!
//! DOLPHIN MUST BE SILENCED. Dolphin publishes its own Discord presence and it is ON by default
//! (`Main.General.UseDiscordPresence`). Two processes publishing for one account race on the local
//! IPC socket and the winner is whichever connected last, so presence flickers between "NFL Street
//! Reborn" and "Dolphin". Every launch this launcher starts must pin that key to False.
//!
//! WHAT IS NEVER PUBLISHED, and why each one:
//!   * A rating. It invites harassment and it is on the public ladder anyway.
//!   * Anything about an OFFLINE game beyond the fact of it - no team, no field, no score. Offline
//!     is private by default; that is a product decision, not a default.
//!   * A match id, a nonce, or a token. A join secret carries a ROOM CODE and nothing else.
//!   * Anything about the player's library or installed content.
//!
//! LIVE SCORE is supported by the shape here ([`Activity::InMatch::score`]) and is `None` until the
//! live scoreboard address is reverse-engineered. The stat tracker reads scores only off the Game
//! Stats screen at the END of a game, so nothing in the tree can supply an in-progress score yet.
//! The design doc argued against publishing a live score at all, as a spoiler and a harassment
//! vector; Adam asked for it explicitly, which overrides that, and the shape is ready for it.

use std::fmt;

/// Which queue or match a player is in. Casual and ranked are separate everywhere else, so they are
/// separate here too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Casual,
    Ranked,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Casual => "Casual",
            Mode::Ranked => "Ranked",
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Where a best-of-three has got to. Casual is a single game and carries none of this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Series {
    pub game: u8,
    pub best_of: u8,
    pub wins_self: u8,
    pub wins_opponent: u8,
}

/// Everything the launcher can be doing, from its own front door to the last snap of a series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// The launcher is open and the player is not doing anything else yet.
    Launcher,
    /// A screen worth naming - Mods, Controls, Settings, Stats. Never names installed content.
    Browsing { page: &'static str },
    /// An offline game. Deliberately opaque: offline is private.
    Offline,
    /// The game is up but no match is running - front end, team select, the pickup pool.
    Menus,
    /// Waiting for an opponent.
    Queued { mode: Mode, start_unix: i64 },
    /// In a lobby, with a room code others can join through Discord.
    Lobby { mode: Mode, code: String, players: u8, capacity: u8 },
    /// Matched, and getting both sides into the game.
    Preparing { mode: Mode, opponent: String, field: Option<String> },
    /// Playing. `score` is `(you, them)` and is `None` until a live score exists.
    InMatch {
        mode: Mode,
        opponent: String,
        field: Option<String>,
        series: Option<Series>,
        score: Option<(u32, u32)>,
        start_unix: i64,
    },
}

/// The flat shape Discord actually wants. No Discord types here, so it can be asserted in a test.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Payload {
    /// The top line.
    pub details: String,
    /// The second line. Empty means "send no state line".
    pub state: String,
    pub large_image: String,
    pub large_text: String,
    pub small_image: String,
    pub small_text: String,
    /// Unix seconds; Discord renders the elapsed counter from it.
    pub start_unix: Option<i64>,
    /// `(current, max)`, only for a lobby.
    pub party: Option<(u8, u8)>,
    /// A room code, never a credential. Discord's Ask to Join hands this back to the launcher.
    pub join_secret: Option<String>,
}

/// The default art asset. Anything else falls back to this rather than showing a broken image.
pub const LOGO: &str = "logo";

/// Whether per-field art has been uploaded to the Discord application.
///
/// FALSE ON PURPOSE. Discord shows NO IMAGE AT ALL when an activity names an asset key that does
/// not exist - it does not fall back - so naming `field_brooklyn` before that asset is uploaded
/// loses the logo too, on exactly the states that matter most. The field is still shown, as the
/// image's hover text, which costs nothing. Flip this to true once the per-field art exists.
pub const USE_FIELD_ART: bool = false;

/// What a lobby's join secret stands for (launcher spec 2026-10-02 §3.10): the same `nflstreetreborn://room/<code>`
/// link a copied invite opens, so Discord's Join and a pasted link land in the same place on the Online page.
pub fn join_link(secret: &str) -> String {
    format!("nflstreetreborn://room/{}", secret.trim())
}

/// The image and its hover text for an activity that knows which field is being played.
///
/// Returns `(asset key, hover text)`. The hover text names the field whether or not its art exists,
/// so turning `USE_FIELD_ART` on later changes the picture and nothing else.
fn field_asset(field: Option<&str>) -> (String, String) {
    let named = field.map(str::trim).filter(|f| !f.is_empty());
    let text = named.map(str::to_string).unwrap_or_else(|| "NFL Street Reborn".to_string());
    match named {
        Some(f) if USE_FIELD_ART => {
            let key: String = f
                .to_ascii_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            (format!("field_{key}"), text)
        }
        _ => (LOGO.to_string(), text),
    }
}

/// Render an activity into what Discord shows. Pure.
pub fn payload(activity: &Activity) -> Payload {
    let base = Payload {
        large_image: LOGO.to_string(),
        large_text: "NFL Street Reborn".to_string(),
        ..Default::default()
    };

    match activity {
        Activity::Launcher => Payload { details: "In the launcher".into(), ..base },

        Activity::Browsing { page } => Payload {
            details: "In the launcher".into(),
            state: (*page).to_string(),
            ..base
        },

        // No team, no field, no score, no timer. The fact of playing offline is all that leaves
        // this machine, because offline games are private by default.
        Activity::Offline => Payload { details: "Playing offline".into(), ..base },

        Activity::Menus => Payload { details: "In the menus".into(), ..base },

        Activity::Queued { mode, start_unix } => Payload {
            details: format!("Looking for a {} game", mode.label().to_ascii_lowercase()),
            state: "In the queue".into(),
            small_image: LOGO.to_string(),
            small_text: mode.label().to_string(),
            start_unix: Some(*start_unix),
            ..base
        },

        Activity::Lobby { mode, code, players, capacity } => Payload {
            details: format!("In a {} lobby", mode.label().to_ascii_lowercase()),
            state: if *players >= *capacity { "Ready to start".into() } else { "Waiting for a player".into() },
            party: Some((*players, *capacity)),
            // The code, and only the code. Never the match id, never a token.
            join_secret: Some(code.clone()),
            ..base
        },

        Activity::Preparing { mode, opponent, field } => {
            let (large_image, large_text) = field_asset(field.as_deref());
            Payload {
                details: format!("{} match", mode.label()),
                state: format!("Starting against {opponent}"),
                large_image,
                large_text,
                ..base
            }
        }

        Activity::InMatch { mode, opponent, field, series, score, start_unix } => {
            let (large_image, large_text) = field_asset(field.as_deref());
            let details = match score {
                // Scores and series: a plain hyphen, as everywhere in the launcher ("14-7", "1-0"; launch audit M11).
                Some((us, them)) => format!("{mode} — {us}-{them} vs {opponent}"),
                None => format!("{mode} — vs {opponent}"),
            };
            let state = match series {
                Some(s) => format!("Game {} of {} ({}-{})", s.game, s.best_of, s.wins_self, s.wins_opponent),
                None => field.clone().unwrap_or_default(),
            };
            Payload {
                details,
                state,
                large_image,
                large_text,
                small_image: LOGO.to_string(),
                small_text: mode.label().to_string(),
                start_unix: Some(*start_unix),
                ..base
            }
        }
    }
}

/// The transport. Implemented over Discord's local IPC by the launcher, and faked in tests.
///
/// Every method returns `Result` because Discord may simply not be running, which is NORMAL and
/// must never block a launch, a queue or a result upload. Callers log and carry on.
pub trait Presence {
    fn set(&mut self, payload: &Payload) -> Result<(), String>;
    fn clear(&mut self) -> Result<(), String>;
}

/// A presence that does nothing, for players who switch it off and for tests.
#[derive(Debug, Default)]
pub struct NoPresence;

impl Presence for NoPresence {
    fn set(&mut self, _payload: &Payload) -> Result<(), String> {
        Ok(())
    }
    fn clear(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_join_secret_stands_for_the_room_link() {
        assert_eq!(join_link("K7QP2M"), "nflstreetreborn://room/K7QP2M");
        assert_eq!(join_link(" K7QP2M "), "nflstreetreborn://room/K7QP2M");
    }

    fn p(a: Activity) -> Payload {
        payload(&a)
    }

    #[test]
    fn an_offline_game_says_nothing_about_itself() {
        // Offline is private by default. Not the team, not the field, not the score, and not even
        // how long it has been going: an elapsed timer is itself a fact about someone's evening.
        let got = p(Activity::Offline);
        assert_eq!(got.details, "Playing offline");
        assert_eq!(got.state, "");
        assert_eq!(got.start_unix, None);
        assert_eq!(got.party, None);
        assert_eq!(got.join_secret, None);
    }

    #[test]
    fn a_rating_is_never_published_anywhere() {
        // There is no rating field, and no copy should ever grow one: it invites harassment and it
        // is on the public ladder anyway. This walks every activity rather than trusting review.
        let all = [
            Activity::Launcher,
            Activity::Browsing { page: "Mods" },
            Activity::Offline,
            Activity::Menus,
            Activity::Queued { mode: Mode::Ranked, start_unix: 1 },
            Activity::Lobby { mode: Mode::Ranked, code: "ABC123".into(), players: 1, capacity: 2 },
            Activity::Preparing { mode: Mode::Ranked, opponent: "Jock".into(), field: None },
            Activity::InMatch {
                mode: Mode::Ranked,
                opponent: "Jock".into(),
                field: Some("Brooklyn".into()),
                series: None,
                score: Some((28, 21)),
                start_unix: 1,
            },
        ];
        for a in all {
            let got = p(a.clone());
            let text = format!("{} {} {} {}", got.details, got.state, got.large_text, got.small_text);
            let lower = text.to_ascii_lowercase();
            assert!(!lower.contains("elo"), "{a:?} leaked a rating word: {text}");
            assert!(!lower.contains("rating"), "{a:?} leaked a rating word: {text}");
            assert!(!lower.contains("mmr"), "{a:?} leaked a rating word: {text}");
        }
    }

    #[test]
    fn a_join_secret_is_the_room_code_and_only_a_lobby_has_one() {
        let lobby = p(Activity::Lobby { mode: Mode::Casual, code: "K7QP2M".into(), players: 1, capacity: 2 });
        assert_eq!(lobby.join_secret.as_deref(), Some("K7QP2M"));
        assert_eq!(lobby.party, Some((1, 2)));

        // A match is not joinable, so it must not advertise one.
        let playing = p(Activity::InMatch {
            mode: Mode::Ranked,
            opponent: "Jock".into(),
            field: None,
            series: None,
            score: None,
            start_unix: 10,
        });
        assert_eq!(playing.join_secret, None);
        assert_eq!(playing.party, None);
    }

    #[test]
    fn the_live_score_is_shown_when_there_is_one_and_omitted_cleanly_when_there_is_not() {
        let with = p(Activity::InMatch {
            mode: Mode::Ranked,
            opponent: "Jock".into(),
            field: Some("Brooklyn".into()),
            series: None,
            score: Some((28, 21)),
            start_unix: 5,
        });
        assert!(with.details.contains("28-21"), "{}", with.details);
        assert!(with.details.contains("21"), "{}", with.details);
        assert!(with.details.contains("Jock"), "{}", with.details);

        // No score yet is not a broken line with an empty gap in it.
        let without = p(Activity::InMatch {
            mode: Mode::Ranked,
            opponent: "Jock".into(),
            field: Some("Brooklyn".into()),
            series: None,
            score: None,
            start_unix: 5,
        });
        assert_eq!(without.details, "Ranked — vs Jock");
        assert!(!without.details.contains('-'), "an absent score must not leave a dash: {}", without.details);
    }

    #[test]
    fn a_series_is_shown_as_a_series() {
        let got = p(Activity::InMatch {
            mode: Mode::Ranked,
            opponent: "Tuna".into(),
            field: Some("The Pit".into()),
            series: Some(Series { game: 2, best_of: 3, wins_self: 1, wins_opponent: 0 }),
            score: Some((14, 6)),
            start_unix: 5,
        });
        assert_eq!(got.state, "Game 2 of 3 (1-0)");
    }

    #[test]
    fn a_field_is_always_named_and_never_asks_for_art_that_does_not_exist() {
        // Discord shows NO image when an activity names a missing asset key, so until per-field art
        // is uploaded the logo must stay the image. The field is still named, in the hover text.
        let known = p(Activity::Preparing { mode: Mode::Casual, opponent: "A".into(), field: Some("City Limits".into()) });
        assert_eq!(known.large_text, "City Limits", "the field is named whether or not it has art");
        if USE_FIELD_ART {
            assert_eq!(known.large_image, "field_city_limits");
        } else {
            assert_eq!(known.large_image, LOGO, "no per-field art uploaded, so the logo must be used");
        }

        for empty in [None, Some(String::new()), Some("   ".to_string())] {
            let got = p(Activity::Preparing { mode: Mode::Casual, opponent: "A".into(), field: empty });
            assert_eq!(got.large_image, LOGO);
            assert_eq!(got.large_text, "NFL Street Reborn");
        }
    }

    #[test]
    fn the_queue_and_the_match_both_carry_an_elapsed_timer_and_the_launcher_does_not() {
        assert_eq!(p(Activity::Queued { mode: Mode::Casual, start_unix: 99 }).start_unix, Some(99));
        assert_eq!(
            p(Activity::InMatch {
                mode: Mode::Casual,
                opponent: "A".into(),
                field: None,
                series: None,
                score: None,
                start_unix: 77
            })
            .start_unix,
            Some(77)
        );
        assert_eq!(p(Activity::Launcher).start_unix, None);
        assert_eq!(p(Activity::Browsing { page: "Settings" }).start_unix, None);
    }

    #[test]
    fn every_activity_produces_a_details_line() {
        // Discord shows nothing at all when details is empty, which reads as "presence is broken".
        for a in [
            Activity::Launcher,
            Activity::Browsing { page: "Mods" },
            Activity::Offline,
            Activity::Menus,
            Activity::Queued { mode: Mode::Casual, start_unix: 1 },
            Activity::Lobby { mode: Mode::Casual, code: "A".into(), players: 1, capacity: 2 },
            Activity::Preparing { mode: Mode::Casual, opponent: "A".into(), field: None },
            Activity::InMatch {
                mode: Mode::Casual,
                opponent: "A".into(),
                field: None,
                series: None,
                score: None,
                start_unix: 1,
            },
        ] {
            assert!(!payload(&a).details.is_empty(), "{a:?} produced no details line");
        }
    }

    #[test]
    fn nothing_ever_publishes_an_identifier_a_player_did_not_choose() {
        // A room code is the one identifier that may leave, because Ask to Join needs it. A match
        // id or a token must never appear, even indirectly through the opponent or field strings.
        let got = p(Activity::InMatch {
            mode: Mode::Ranked,
            opponent: "Jock".into(),
            field: Some("Brooklyn".into()),
            series: None,
            score: None,
            start_unix: 1,
        });
        assert_eq!(got.join_secret, None);
        let all = format!("{got:?}");
        assert!(!all.contains("match_id"), "{all}");
        assert!(!all.contains("nonce"), "{all}");
        assert!(!all.contains("token"), "{all}");
    }
}
