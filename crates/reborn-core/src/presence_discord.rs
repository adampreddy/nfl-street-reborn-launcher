//! The half of presence that can fail: Discord's local IPC socket.
//!
//! Kept apart from [`crate::presence`] so the copy and the privacy rules stay testable without a
//! running Discord. Everything here is best-effort by design.
//!
//! DISCORD NOT RUNNING IS THE NORMAL CASE, not an error. Plenty of players will never have it open.
//! So every call returns `Result` and every caller is expected to log and carry on: presence must
//! never block a launch, a queue, or a result upload. The client also reconnects lazily rather than
//! holding a connection open, because Discord can be started and stopped underneath us at any time.

use std::time::{SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity as RpcActivity, Assets, Button, Party, Secrets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

use crate::presence::{Payload, Presence};

/// The Reborn Discord application. Public - an application id is not a credential, which is why it
/// can live in source rather than in the vault. The bot token and the OAuth client secret, which
/// ARE credentials, are read from the environment and never appear here.
pub const DEFAULT_APP_ID: &str = "1547132125332115587";

/// Discord's own limits. A field over the limit is REJECTED by Discord, taking the whole update
/// with it, so everything is truncated here rather than discovering it as a silent no-op.
const MAX_FIELD: usize = 128;

fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_FIELD {
        return s.to_string();
    }
    // Truncate on a character boundary, never a byte one: a field name can hold a player's display
    // name, and half a multi-byte character is a field Discord refuses outright.
    let mut out: String = s.chars().take(MAX_FIELD - 1).collect();
    out.push('\u{2026}');
    out
}

/// Unix seconds now. Used only when a caller did not supply its own start time.
pub fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// A presence backed by Discord's local IPC socket.
pub struct DiscordPresence {
    app_id: String,
    client: Option<DiscordIpcClient>,
    /// The last payload actually accepted, so an unchanged state is not re-sent every tick.
    last: Option<Payload>,
}

impl std::fmt::Debug for DiscordPresence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The IPC client is not Debug, and whether it is connected is the only fact worth showing.
        f.debug_struct("DiscordPresence")
            .field("app_id", &self.app_id)
            .field("connected", &self.client.is_some())
            .finish()
    }
}

impl DiscordPresence {
    pub fn new(app_id: impl Into<String>) -> Self {
        Self { app_id: app_id.into(), client: None, last: None }
    }

    pub fn with_default_app() -> Self {
        Self::new(DEFAULT_APP_ID)
    }

    /// Connect if not already connected. A failure here is ordinary - Discord may not be running.
    fn ensure(&mut self) -> Result<&mut DiscordIpcClient, String> {
        if self.client.is_none() {
            // new() cannot fail - it only records the id. connect() is where Discord is reached.
            let mut c = DiscordIpcClient::new(&self.app_id);
            c.connect().map_err(|e| format!("presence connect: {e}"))?;
            self.client = Some(c);
        }
        self.client.as_mut().ok_or_else(|| "presence client missing".to_string())
    }

    /// Drop the connection so the next call reconnects. Called whenever a send fails, because a
    /// half-dead socket reports success and shows nothing, which is worse than a visible retry.
    fn reset(&mut self) {
        if let Some(mut c) = self.client.take() {
            let _ = c.close();
        }
        self.last = None;
    }
}

impl Presence for DiscordPresence {
    fn set(&mut self, payload: &Payload) -> Result<(), String> {
        if self.last.as_ref() == Some(payload) {
            return Ok(()); // Nothing changed; do not spend a round trip or risk Discord's rate limit.
        }

        let details = clip(&payload.details);
        let state = clip(&payload.state);
        let large_image = clip(&payload.large_image);
        let large_text = clip(&payload.large_text);
        let small_image = clip(&payload.small_image);
        let small_text = clip(&payload.small_text);
        let secret = payload.join_secret.as_deref().map(clip);

        let result = (|| -> Result<(), String> {
            let client = self.ensure()?;

            let mut assets = Assets::new();
            if !large_image.is_empty() {
                assets = assets.large_image(&large_image);
            }
            if !large_text.is_empty() {
                assets = assets.large_text(&large_text);
            }
            if !small_image.is_empty() {
                assets = assets.small_image(&small_image);
            }
            if !small_text.is_empty() {
                assets = assets.small_text(&small_text);
            }

            let mut activity = RpcActivity::new().details(&details).assets(assets);
            if !state.is_empty() {
                activity = activity.state(&state);
            }
            if let Some(start) = payload.start_unix {
                activity = activity.timestamps(Timestamps::new().start(start));
            }
            if let Some((cur, max)) = payload.party {
                activity = activity.party(Party::new().size([cur as i32, max as i32]));
            }
            if let Some(s) = secret.as_deref() {
                // A join secret is a LOBBY CODE. It stands for presence::join_link(code), the same
                // nflstreetreborn://room/<code> link a copied invite opens (never join/, which is a
                // Netplay host code).
                activity = activity.secrets(Secrets::new().join(s));
            }
            activity = activity.buttons(vec![Button::new("Get NFL Street Reborn", "https://nflstreetreborn.com")]);

            client.set_activity(activity).map_err(|e| format!("presence set: {e}"))
        })();

        match result {
            Ok(()) => {
                self.last = Some(payload.clone());
                Ok(())
            }
            Err(e) => {
                self.reset();
                Err(e)
            }
        }
    }

    fn clear(&mut self) -> Result<(), String> {
        self.last = None;
        let Some(client) = self.client.as_mut() else { return Ok(()) };
        let r = client.clear_activity().map_err(|e| format!("presence clear: {e}"));
        if r.is_err() {
            self.reset();
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presence::{payload, Activity, Mode};

    #[test]
    fn the_application_id_is_the_reborn_one_and_is_not_a_secret() {
        // An application id is public by construction - it is broadcast to every Discord client
        // that sees the presence, which is why it can sit in source. The bot token and the OAuth
        // client secret are the credentials and are read from the environment instead.
        //
        // An earlier version of this test scanned THIS FILE for credential-shaped words and failed
        // on its own word list. A guard that its own text can trip is not a guard; assert the shape
        // of the constant instead, which nothing else in the file can satisfy by accident.
        assert_eq!(DEFAULT_APP_ID, "1547132125332115587");
        assert!(DEFAULT_APP_ID.chars().all(|c| c.is_ascii_digit()), "an app id is a bare snowflake");
        assert!(!DEFAULT_APP_ID.contains('.'), "a dotted value here would be a bot token, not an app id");
    }

    #[test]
    fn an_over_long_field_is_clipped_on_a_character_boundary() {
        // Discord REJECTS an over-long field and drops the whole update with it, so this is the
        // difference between a truncated name and no presence at all.
        let long = "\u{00e9}".repeat(400); // multi-byte on purpose
        let out = clip(&long);
        assert!(out.chars().count() <= MAX_FIELD, "clipped to {} chars", out.chars().count());
        assert!(out.ends_with('\u{2026}'));
        assert!(std::str::from_utf8(out.as_bytes()).is_ok(), "clipping must not split a character");

        let short = "Brooklyn";
        assert_eq!(clip(short), short, "a short field is passed through untouched");
    }

    #[test]
    fn a_payload_survives_the_round_trip_into_the_fields_discord_wants() {
        // Not a Discord call - just proof the mapping reads the fields it is supposed to and that
        // a lobby is the only thing carrying a join secret.
        let lobby = payload(&Activity::Lobby {
            mode: Mode::Casual,
            code: "K7QP2M".into(),
            players: 1,
            capacity: 2,
        });
        assert_eq!(lobby.join_secret.as_deref(), Some("K7QP2M"));
        assert_eq!(lobby.party, Some((1, 2)));
        assert!(!clip(&lobby.details).is_empty());
    }

    #[test]
    fn an_unchanged_payload_is_not_re_sent() {
        // The UI ticks this several times a second. Without the guard every tick is a round trip
        // and Discord rate-limits the connection away.
        let mut p = DiscordPresence::new("0");
        let body = payload(&Activity::Launcher);
        p.last = Some(body.clone());
        assert!(p.set(&body).is_ok(), "an identical payload must short-circuit, not connect");
        assert!(p.client.is_none(), "short-circuiting must not have opened a connection");
    }

    #[test]
    fn a_failed_send_forgets_the_connection_and_the_state() {
        // NOT written by connecting to a bogus app id: that depends on whether Discord happens to
        // be running on the machine running the tests, and it duly passed alone and failed in the
        // full suite. The behaviour that matters is deterministic and can be asserted directly -
        // after a failure the client is dropped so the next call reconnects, and `last` is cleared
        // so an unchanged payload is not short-circuited against a state Discord never accepted.
        // Live connection behaviour is covered by examples/presence_probe.rs against real Discord.
        let mut p = DiscordPresence::new(DEFAULT_APP_ID);
        p.last = Some(payload(&Activity::Launcher));
        p.reset();
        assert!(p.client.is_none(), "a reset must drop the connection so the next call retries");
        assert!(p.last.is_none(), "a reset must not leave a state Discord may never have accepted");
    }

}
