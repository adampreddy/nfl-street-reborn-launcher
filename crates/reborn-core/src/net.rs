//! The one place network code lives.
//!
//! Everything that talks to a server goes through [`Http`], and everything
//! that uses it takes `&dyn Http`, so a test hands in a fake and never opens a
//! socket. The real implementation is [`UreqHttp`], a blocking client with a
//! timeout on every call, because a launcher that hangs on a dead server is
//! indistinguishable from a launcher that crashed.
//!
//! No URL and no credential is defined here. Those belong to the feature that
//! uses them, beside the rules for what a reply means.

use std::path::Path;
use std::time::Duration;

use crate::{Error, Result};

/// A reply the server actually sent, whatever its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// The network, as three calls.
///
/// A transport failure (no route, DNS, timeout, refused) is an `Err`. A reply
/// with an error status is an `Ok(HttpResponse)`, because the server said
/// something and the caller decides what a 409 or a 422 means.
pub trait Http: Send + Sync {
    /// GET a small text body, such as a manifest.
    fn get_text(&self, url: &str) -> Result<HttpResponse>;

    /// GET a large body straight to a file. Returns the bytes written. The
    /// file is created or truncated; the caller owns naming and cleanup.
    fn get_to_file(&self, url: &str, to: &Path) -> Result<u64>;

    /// [`Http::get_to_file`] with an `Authorization: Bearer` header, for an
    /// endpoint that needs a login, such as the site's roster export. The
    /// token is never logged by this layer. The default refuses a token
    /// rather than silently dropping it, so an implementation that has not
    /// been taught to send one cannot look like it did.
    fn get_to_file_as(&self, url: &str, to: &Path, bearer: Option<&str>) -> Result<u64> {
        match bearer {
            None => self.get_to_file(url, to),
            Some(_) => Err(Error::PlainIo(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("{url}: this HTTP client cannot send a bearer token"),
            ))),
        }
    }

    /// POST a JSON body. `bearer` becomes an `Authorization: Bearer` header
    /// and is never logged by this layer.
    fn post_json(&self, url: &str, body: &[u8], bearer: Option<&str>) -> Result<HttpResponse>;
}

fn transport(url: &str, e: impl std::fmt::Display) -> Error {
    Error::PlainIo(std::io::Error::new(
        std::io::ErrorKind::NotConnected,
        format!("{url} could not be reached: {e}"),
    ))
}

/// The real client.
#[derive(Debug)]
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    /// `user_agent` names the launcher and its version. Required rather than
    /// cosmetic: the league endpoint sits behind a host that refuses requests
    /// with no User-Agent, which Dolphin's uploader learned the hard way.
    pub fn new(user_agent: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .user_agent(user_agent)
            .build();
        Self { agent }
    }

    fn reply(url: &str, outcome: std::result::Result<ureq::Response, ureq::Error>) -> Result<HttpResponse> {
        match outcome {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.into_string().map_err(|e| transport(url, e))?;
                Ok(HttpResponse { status, body })
            }
            // The server answered with an error status. That is a reply, and
            // the body usually says why.
            Err(ureq::Error::Status(status, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                Ok(HttpResponse { status, body })
            }
            Err(ureq::Error::Transport(t)) => Err(transport(url, t)),
        }
    }
}

impl Http for UreqHttp {
    fn get_text(&self, url: &str) -> Result<HttpResponse> {
        Self::reply(url, self.agent.get(url).call())
    }

    fn get_to_file(&self, url: &str, to: &Path) -> Result<u64> {
        let resp = match self.agent.get(url).call() {
            Ok(resp) => resp,
            Err(ureq::Error::Status(status, _)) => {
                return Err(Error::PlainIo(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{url} answered {status}"),
                )))
            }
            Err(ureq::Error::Transport(t)) => return Err(transport(url, t)),
        };
        let mut file = std::fs::File::create(to).map_err(|source| Error::Io {
            path: to.to_path_buf(),
            source,
        })?;
        let mut reader = resp.into_reader();
        std::io::copy(&mut reader, &mut file).map_err(|source| Error::Io {
            path: to.to_path_buf(),
            source,
        })
    }

    fn get_to_file_as(&self, url: &str, to: &Path, bearer: Option<&str>) -> Result<u64> {
        let mut req = self.agent.get(url);
        if let Some(token) = bearer {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        let resp = match req.call() {
            Ok(resp) => resp,
            Err(ureq::Error::Status(status, _)) => {
                return Err(Error::PlainIo(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{url} answered {status}"),
                )))
            }
            Err(ureq::Error::Transport(t)) => return Err(transport(url, t)),
        };
        let mut file = std::fs::File::create(to).map_err(|source| Error::Io {
            path: to.to_path_buf(),
            source,
        })?;
        let mut reader = resp.into_reader();
        std::io::copy(&mut reader, &mut file).map_err(|source| Error::Io {
            path: to.to_path_buf(),
            source,
        })
    }

    fn post_json(&self, url: &str, body: &[u8], bearer: Option<&str>) -> Result<HttpResponse> {
        let mut req = self
            .agent
            .post(url)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json");
        if let Some(token) = bearer {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        Self::reply(url, req.send_bytes(body))
    }
}

/// A fake for tests: canned replies by URL, and a record of what was sent.
#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// (url, body, bearer)
    pub type Post = (String, Vec<u8>, Option<String>);

    #[derive(Debug, Default)]
    pub struct FakeHttp {
        /// URL -> (status, body). A URL not listed is a transport failure.
        pub replies: BTreeMap<String, (u16, Vec<u8>)>,
        /// Every POST, in order: (url, body, bearer).
        pub posts: Mutex<Vec<Post>>,
    }

    impl FakeHttp {
        pub fn reply(mut self, url: &str, status: u16, body: impl AsRef<[u8]>) -> Self {
            self.replies.insert(url.to_string(), (status, body.as_ref().to_vec()));
            self
        }
    }

    impl Http for FakeHttp {
        fn get_text(&self, url: &str) -> Result<HttpResponse> {
            match self.replies.get(url) {
                Some((status, body)) => Ok(HttpResponse {
                    status: *status,
                    body: String::from_utf8_lossy(body).to_string(),
                }),
                None => Err(transport(url, "no route in the fake")),
            }
        }

        fn get_to_file(&self, url: &str, to: &Path) -> Result<u64> {
            match self.replies.get(url) {
                Some((status, body)) if (200..300).contains(status) => {
                    std::fs::write(to, body).map_err(|source| Error::Io {
                        path: to.to_path_buf(),
                        source,
                    })?;
                    Ok(body.len() as u64)
                }
                Some((status, _)) => Err(Error::PlainIo(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{url} answered {status}"),
                ))),
                None => Err(transport(url, "no route in the fake")),
            }
        }

        fn post_json(&self, url: &str, body: &[u8], bearer: Option<&str>) -> Result<HttpResponse> {
            self.posts
                .lock()
                .unwrap()
                .push((url.to_string(), body.to_vec(), bearer.map(str::to_string)));
            self.get_text(url)
        }
    }
}
