//! iTunes and MusicBrainz song lookup through the Http trait.
//!
//! Both are free, need no key, and return title, artist, album, and length.
//!
//! # Why this ranks and filters rather than listing what came back
//!
//! A file name is a bad query. Ask MusicBrainz for "4 26 Z89 Cast" and it
//! answers, confidently, with Czech audiobook chapters: its search never says
//! "nothing", it says "here is the closest thing in the database". Showing
//! that list is worse than showing none, because a plausible wrong answer is
//! the one a person clicks.
//!
//! So a lookup here is three steps, not one. It reads what structure the file
//! name has ("Artist - Title" is a real convention, see
//! [`crate::soundtrack::tags::parse_name`]) and asks with it. It scores every
//! candidate against what was asked, including against the file's true length,
//! which is the one fact about the song that cannot be spelled wrong. Then it
//! drops everything under [`FLOOR`], so a bad query returns nothing and the
//! page says to type the details.
//!
//! A lookup never errors: no network is an empty list.

use crate::net::Http;

/// The score a candidate must reach to be worth showing. Tuned so a title or
/// artist match with the right length passes and a database's nearest miss
/// does not.
pub const FLOOR: f64 = 0.5;

/// MusicBrainz's own relevance, below which its answer is a guess.
const MB_MIN_RELEVANCE: u64 = 60;

/// How far a candidate's length may sit from the file's before the length
/// stops counting in its favour, as a fraction of the file's length.
const DURATION_TOLERANCE: f64 = 0.20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: Option<u64>,
    /// "iTunes" or "MusicBrainz".
    pub source: &'static str,
}

/// What is known about the song being looked up.
#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    /// The whole cleaned name, always searched.
    pub text: &'a str,
    /// The halves of an "Artist - Title" name, when it had them.
    pub artist: Option<&'a str>,
    pub title: Option<&'a str>,
    /// The file's real length, which outranks anything a name suggests.
    pub duration_ms: Option<u64>,
}

impl<'a> Request<'a> {
    pub fn from_text(text: &'a str) -> Self {
        Self { text, ..Self::default() }
    }
}

/// RFC 3986 percent-encoding: unreserved bytes pass, everything else `%XX`.
pub fn encode(query: &str) -> String {
    let mut out = String::with_capacity(query.len() * 3);
    for b in query.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn itunes_url(query: &str) -> String {
    format!("https://itunes.apple.com/search?media=music&entity=song&limit=8&term={}", encode(query))
}

pub fn musicbrainz_url(query: &str) -> String {
    format!("https://musicbrainz.org/ws/2/recording?fmt=json&limit=8&query={}", encode(query))
}

/// A MusicBrainz query that names the fields, for a file name that had them.
/// Far better than the same words as loose text: it stops a title matching an
/// artist and every other accident of a full-text index.
pub fn musicbrainz_fielded_url(artist: &str, title: &str) -> String {
    let quoted = |s: &str| format!("\"{}\"", s.replace('"', " "));
    musicbrainz_url(&format!("recording:{} AND artist:{}", quoted(title), quoted(artist)))
}

fn text(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").trim().to_string()
}

pub fn parse_itunes(json: &str) -> Vec<Candidate> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(results) = v.get("results").and_then(|r| r.as_array()) else {
        return Vec::new();
    };
    results
        .iter()
        .filter_map(|r| {
            let title = text(r, "trackName");
            if title.is_empty() {
                return None;
            }
            Some(Candidate {
                title,
                artist: text(r, "artistName"),
                album: text(r, "collectionName"),
                duration_ms: r.get("trackTimeMillis").and_then(|x| x.as_u64()),
                source: "iTunes",
            })
        })
        .collect()
}

/// MusicBrainz recordings, minus the ones it scored as a guess.
pub fn parse_musicbrainz(json: &str) -> Vec<Candidate> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(recs) = v.get("recordings").and_then(|r| r.as_array()) else {
        return Vec::new();
    };
    recs.iter()
        .filter_map(|r| {
            let title = text(r, "title");
            if title.is_empty() {
                return None;
            }
            // Absent means an older response shape, not a weak match.
            if let Some(score) = r.get("score").and_then(|x| x.as_u64()) {
                if score < MB_MIN_RELEVANCE {
                    return None;
                }
            }
            let artist = r
                .get("artist-credit")
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .map(|a| text(a, "name"))
                .unwrap_or_default();
            let album = r
                .get("releases")
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .map(|a| text(a, "title"))
                .unwrap_or_default();
            Some(Candidate { title, artist, album, duration_ms: r.get("length").and_then(|x| x.as_u64()), source: "MusicBrainz" })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Ranking
// ---------------------------------------------------------------------------

/// Lowercase letters and digits only, so punctuation and spacing stop
/// mattering: "Un-thinkable (I'm Ready)" and "unthinkable i'm ready" are the
/// same string to compare.
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(|t| t.to_lowercase())
        .collect()
}

/// How much of `want` is present in `have`, 0 to 1.
fn overlap(want: &[String], have: &[String]) -> f64 {
    if want.is_empty() {
        return 0.0;
    }
    let hit = want.iter().filter(|w| have.contains(w)).count();
    hit as f64 / want.len() as f64
}

/// How well one field matches: containment either way is a full match,
/// otherwise how many of the wanted words are there.
fn field_match(want: &str, have: &str) -> f64 {
    let (w, h) = (norm(want), norm(have));
    if w.is_empty() || h.is_empty() {
        return 0.0;
    }
    if w.contains(&h) || h.contains(&w) {
        return 1.0;
    }
    overlap(&tokens(want), &tokens(have))
}

/// A candidate's fitness for a request, 0 to 1.
///
/// Length is a quarter of it, and counts as neutral rather than against when
/// either side does not know it: MusicBrainz often has no length, and
/// punishing that would bury the only source that knows about a B-side.
pub fn score(req: &Request<'_>, c: &Candidate) -> f64 {
    let base = match (req.title, req.artist) {
        (Some(t), Some(a)) if !t.is_empty() && !a.is_empty() => {
            0.65 * field_match(t, &c.title) + 0.35 * field_match(a, &c.artist)
        }
        _ => {
            let whole = format!("{} {}", c.title, c.artist);
            let by_token = overlap(&tokens(req.text), &tokens(&whole));
            let by_string = if norm(&whole).contains(&norm(req.text)) { 1.0 } else { 0.0 };
            by_token.max(by_string)
        }
    };
    let length = match (req.duration_ms, c.duration_ms) {
        (Some(want), Some(have)) if want > 0 => {
            let off = (want as f64 - have as f64).abs() / want as f64;
            (1.0 - off / DURATION_TOLERANCE).clamp(0.0, 1.0)
        }
        _ => 0.5,
    };
    0.75 * base + 0.25 * length
}

/// Rank, drop the guesses, and remove duplicates of the same recording.
fn rank(req: &Request<'_>, found: Vec<Candidate>) -> Vec<Candidate> {
    let mut scored: Vec<(f64, Candidate)> =
        found.into_iter().map(|c| (score(req, &c), c)).filter(|(s, _)| *s >= FLOOR).collect();
    // Stable, so an equal score keeps the order the service chose.
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (_, c) in scored {
        let key = (norm(&c.title), norm(&c.artist));
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(c);
        if out.len() == 8 {
            break;
        }
    }
    out
}

fn get(http: &dyn Http, url: &str) -> Option<String> {
    match http.get_text(url) {
        Ok(r) if r.is_success() => Some(r.body),
        _ => None,
    }
}

/// Look a song up, best first. Never errors; empty means nothing was worth
/// showing, which is a real answer and not a failure.
pub fn search(http: &dyn Http, req: &Request<'_>) -> Vec<Candidate> {
    if req.text.trim().is_empty() {
        return Vec::new();
    }
    let mut found = get(http, &itunes_url(req.text)).map(|b| parse_itunes(&b)).unwrap_or_default();
    let mut best = rank(req, found.clone());
    // iTunes answered well enough. Asking MusicBrainz too would only add
    // slower duplicates.
    if best.first().map(|c| score(req, c)).unwrap_or(0.0) >= 0.85 {
        return best;
    }
    let url = match (req.artist, req.title) {
        (Some(a), Some(t)) if !a.is_empty() && !t.is_empty() => musicbrainz_fielded_url(a, t),
        _ => musicbrainz_url(req.text),
    };
    if let Some(body) = get(http, &url) {
        found.extend(parse_musicbrainz(&body));
        best = rank(req, found);
    }
    best
}

/// Look up plain text, with no structure and no known length.
pub fn lookup(http: &dyn Http, query: &str) -> Vec<Candidate> {
    search(http, &Request::from_text(query.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::fake::FakeHttp;

    const ITUNES: &str = r#"{"resultCount":2,"results":[
        {"trackName":"Unthinkable (I'm Ready)","artistName":"Alicia Keys","collectionName":"The Element of Freedom","trackTimeMillis":250000},
        {"trackName":"Unthinkable (I'm Ready) [Remix]","artistName":"Alicia Keys & Drake","collectionName":"Remixes","trackTimeMillis":282000}
    ]}"#;

    const MB: &str = r#"{"recordings":[
        {"title":"Scarred","length":186000,"artist-credit":[{"name":"Mase","artist":{"name":"Mase"}}],"releases":[{"title":"Welcome Back"}]},
        {"title":"Scarred (live)","artist-credit":[]}
    ]}"#;

    #[test]
    fn encodes_the_query_for_a_url() {
        assert_eq!(encode("Alicia Keys Unthinkable (I'm Ready)"), "Alicia%20Keys%20Unthinkable%20%28I%27m%20Ready%29");
        assert_eq!(encode("a-b_c.d~e"), "a-b_c.d~e");
        assert!(itunes_url("x y").ends_with("term=x%20y"));
        assert!(musicbrainz_url("x").ends_with("query=x"));
        assert!(musicbrainz_fielded_url("Nas", "Disciple").contains("recording%3A%22Disciple%22%20AND%20artist%3A%22Nas%22"));
    }

    #[test]
    fn itunes_results_come_back_in_order_with_length() {
        let http = FakeHttp::default().reply(&itunes_url("alicia keys unthinkable"), 200, ITUNES);
        let found = lookup(&http, "alicia keys unthinkable");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].title, "Unthinkable (I'm Ready)");
        assert_eq!(found[0].artist, "Alicia Keys");
        assert_eq!(found[0].album, "The Element of Freedom");
        assert_eq!(found[0].duration_ms, Some(250_000));
        assert_eq!(found[0].source, "iTunes");
        assert_eq!(found[1].artist, "Alicia Keys & Drake");
    }

    #[test]
    fn empty_itunes_falls_through_to_musicbrainz() {
        let http = FakeHttp::default()
            .reply(&itunes_url("scarred"), 200, r#"{"resultCount":0,"results":[]}"#)
            .reply(&musicbrainz_url("scarred"), 200, MB);
        let found = lookup(&http, "scarred");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].title, "Scarred");
        assert_eq!(found[0].artist, "Mase");
        assert_eq!(found[0].album, "Welcome Back");
        assert_eq!(found[0].source, "MusicBrainz");
    }

    #[test]
    fn both_failing_is_empty_and_so_is_an_empty_query() {
        assert!(lookup(&FakeHttp::default(), "anything").is_empty());
        assert!(lookup(&FakeHttp::default(), "   ").is_empty());
    }

    /// The failure that made this worth rewriting: a podcast file name asked
    /// MusicBrainz, which answered with Czech audiobook chapters.
    #[test]
    fn a_name_that_is_not_a_song_returns_nothing_rather_than_the_nearest_thing() {
        const AUDIOBOOK: &str = r#"{"recordings":[
            {"title":"4. Část","score":58,"artist-credit":[{"name":"Zdeněk Izer"}],"releases":[{"title":"Nemocnice na kraji vesnice 2"}]},
            {"title":"Část 4","score":55,"artist-credit":[{"name":"Václav Šorel"}],"releases":[{"title":"Vzpoura mozků"}]}
        ]}"#;
        let q = "4 26 Z89 Cast";
        let http = FakeHttp::default()
            .reply(&itunes_url(q), 200, r#"{"resultCount":0,"results":[]}"#)
            .reply(&musicbrainz_url(q), 200, AUDIOBOOK);
        assert!(lookup(&http, q).is_empty(), "a weak MusicBrainz score is a guess, not an answer");
    }

    #[test]
    fn a_wrong_length_loses_to_the_right_one() {
        let req = Request { text: "alicia keys unthinkable", duration_ms: Some(282_000), ..Request::default() };
        let http = FakeHttp::default().reply(&itunes_url(req.text), 200, ITUNES);
        let found = search(&http, &req);
        assert_eq!(found[0].title, "Unthinkable (I'm Ready) [Remix]", "the 4:42 file is the 4:42 recording");
        assert_eq!(found[1].title, "Unthinkable (I'm Ready)");
    }

    #[test]
    fn a_structured_name_asks_musicbrainz_by_field() {
        const HIT: &str = r#"{"recordings":[
            {"title":"Disciple","score":100,"length":181000,"artist-credit":[{"name":"Nas"}],"releases":[{"title":"Street's Disciple"}]}
        ]}"#;
        let req = Request { text: "Nas Disciple", artist: Some("Nas"), title: Some("Disciple"), duration_ms: Some(180_000) };
        let http = FakeHttp::default()
            .reply(&itunes_url(req.text), 200, r#"{"resultCount":0,"results":[]}"#)
            .reply(&musicbrainz_fielded_url("Nas", "Disciple"), 200, HIT);
        let found = search(&http, &req);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Disciple");
        assert!(score(&req, &found[0]) > 0.9, "an exact title, artist and length");
    }

    #[test]
    fn the_same_recording_from_both_services_is_listed_once() {
        let req = Request::from_text("scarred");
        let itunes = r#"{"results":[{"trackName":"Scarred","artistName":"Mase","collectionName":"Welcome Back"}]}"#;
        let http = FakeHttp::default()
            .reply(&itunes_url("scarred"), 200, itunes)
            .reply(&musicbrainz_url("scarred"), 200, MB);
        let found = search(&http, &req);
        assert_eq!(found.iter().filter(|c| norm(&c.title) == "scarred").count(), 1);
    }

    #[test]
    fn scoring_reads_the_way_it_is_described() {
        let exact = Candidate { title: "Disciple".into(), artist: "Nas".into(), album: String::new(), duration_ms: Some(180_000), source: "iTunes" };
        let req = Request { text: "Nas Disciple", artist: Some("Nas"), title: Some("Disciple"), duration_ms: Some(180_000) };
        assert!((score(&req, &exact) - 1.0).abs() < 1e-9);

        // Right song, no length known anywhere: still well clear of the floor.
        let no_len = Candidate { duration_ms: None, ..exact.clone() };
        assert!(score(&req, &no_len) > FLOOR);

        // Right length, wrong song: not enough on its own.
        let wrong = Candidate { title: "Something Else".into(), artist: "Another Band".into(), ..exact.clone() };
        assert!(score(&req, &wrong) < FLOOR);
    }
}
