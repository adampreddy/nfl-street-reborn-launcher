//! iTunes and MusicBrainz song lookup through the Http trait.
//!
//! Both are free, need no key, and return title, artist, album, and length.
//! iTunes first because it answers fastest and its matching is forgiving of
//! file-name queries; MusicBrainz when iTunes has nothing. A lookup never
//! errors: no network is an empty list, and the page says so in one line.

use crate::net::Http;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: Option<u64>,
    /// "iTunes" or "MusicBrainz".
    pub source: &'static str,
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
            let artist = text(r, "artistName");
            if title.is_empty() {
                return None;
            }
            Some(Candidate {
                title,
                artist,
                album: text(r, "collectionName"),
                duration_ms: r.get("trackTimeMillis").and_then(|x| x.as_u64()),
                source: "iTunes",
            })
        })
        .collect()
}

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
            Some(Candidate {
                title,
                artist,
                album,
                duration_ms: r.get("length").and_then(|x| x.as_u64()),
                source: "MusicBrainz",
            })
        })
        .collect()
}

/// Up to eight candidates for a query. Never errors.
pub fn lookup(http: &dyn Http, query: &str) -> Vec<Candidate> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    if let Ok(r) = http.get_text(&itunes_url(query)) {
        if r.is_success() {
            let found = parse_itunes(&r.body);
            if !found.is_empty() {
                return found;
            }
        }
    }
    match http.get_text(&musicbrainz_url(query)) {
        Ok(r) if r.is_success() => parse_musicbrainz(&r.body),
        _ => Vec::new(),
    }
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
        assert_eq!(found[0].duration_ms, Some(186_000));
        assert_eq!(found[0].source, "MusicBrainz");
        assert_eq!(found[1].artist, "");
        assert_eq!(found[1].duration_ms, None);
    }

    #[test]
    fn a_failing_itunes_also_falls_through() {
        let http = FakeHttp::default()
            .reply(&itunes_url("scarred"), 503, "busy")
            .reply(&musicbrainz_url("scarred"), 200, MB);
        assert_eq!(lookup(&http, "scarred").len(), 2);
        // a transport failure on iTunes (no route in the fake) likewise
        let http = FakeHttp::default().reply(&musicbrainz_url("scarred"), 200, MB);
        assert_eq!(lookup(&http, " scarred ").len(), 2, "the query is trimmed");
    }

    #[test]
    fn both_failing_is_empty_and_so_is_an_empty_query() {
        let http = FakeHttp::default();
        assert!(lookup(&http, "anything").is_empty());
        assert!(lookup(&http, "   ").is_empty());
        let http = FakeHttp::default().reply(&itunes_url("x"), 200, "not json").reply(&musicbrainz_url("x"), 200, "{}");
        assert!(lookup(&http, "x").is_empty());
        assert!(parse_itunes("garbage").is_empty());
        assert!(parse_musicbrainz("[]").is_empty());
    }
}
