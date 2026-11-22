//! The core's client for eve's private session API. eve is untrusted: every answer is parsed
//! into checked types, and nothing it says decides access.
use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt;
use std::time::Duration;
use url::Url;

// Well below the web API's 30 s timeout, so a slow eve fails a request instead of the API cutting it.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
// eve sends heartbeats while a stream is quiet, so a silent read this long means it is gone.
const READ_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum EveError {
    #[error("eve request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("eve answered {0}")]
    Status(u16),
    #[error("eve sent a malformed answer")]
    Malformed,
}

/// Opaque eve session id; never shown to the browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EveSessionId(String);

/// An id eve chose (session or interaction): 1 to 128 of ascii letters, digits, `_`, `-`.
fn eve_id(s: &str) -> Result<(), EveError> {
    let ok = !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    ok.then_some(()).ok_or(EveError::Malformed)
}

impl EveSessionId {
    pub fn parse(s: &str) -> Result<Self, EveError> {
        eve_id(s)?;
        Ok(Self(s.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EveSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One event of a stored stream line.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Fact {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// A stored line at `index`; a progress record is a line with no facts.
    Line { index: u64, facts: Vec<Fact> },
    /// The session ended and its stream is closed: do not reconnect.
    Ended,
}

#[derive(Clone)]
pub struct EveClient {
    http: reqwest::Client,
    base: Url,
}

impl EveClient {
    /// Fails if the HTTP client cannot be built (no TLS backend): startup stops rather than run a
    /// client without these timeouts and redirect rules.
    pub fn new(base: Url) -> Result<Self, EveError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(READ_TIMEOUT)
            // eve is private: a redirect is never expected and never followed.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(EveError::Request)?;
        Ok(Self { http, base })
    }

    fn url(&self, path: &str) -> Result<Url, EveError> {
        self.base.join(path).map_err(|_| EveError::Malformed)
    }

    async fn post(&self, assertion: &str, path: &str, body: Value) -> Result<Value, EveError> {
        let res = self
            .http
            .post(self.url(path)?)
            .bearer_auth(assertion)
            .timeout(CALL_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(EveError::Request)?;
        if !res.status().is_success() {
            return Err(EveError::Status(res.status().as_u16()));
        }
        res.json().await.map_err(|_| EveError::Malformed)
    }

    /// Creates a session and starts its first turn with `message`. eve creates one session per
    /// caller and `operation`: a repeat returns that session without sending the message again.
    pub async fn start(
        &self,
        assertion: &str,
        message: &str,
        operation: &str,
    ) -> Result<EveSessionId, EveError> {
        let body = json!({ "message": message, "operationId": operation });
        let v = self.post(assertion, "/eve/v1/session", body).await?;
        EveSessionId::parse(v["sessionId"].as_str().ok_or(EveError::Malformed)?)
    }

    pub async fn send(
        &self,
        assertion: &str,
        session: &EveSessionId,
        message: &str,
    ) -> Result<(), EveError> {
        let path = format!("/eve/v1/session/{session}");
        self.post(assertion, &path, json!({ "message": message }))
            .await
            .map(drop)
    }

    /// Answers an approval eve is waiting on.
    pub async fn answer(
        &self,
        assertion: &str,
        session: &EveSessionId,
        interaction: &str,
        approve: bool,
    ) -> Result<(), EveError> {
        eve_id(interaction)?;
        let option = if approve { "approve" } else { "deny" };
        let path = format!("/eve/v1/session/{session}");
        let body = json!({ "answers": [{ "interactionId": interaction, "optionId": option }] });
        self.post(assertion, &path, body).await.map(drop)
    }

    pub async fn cancel(&self, assertion: &str, session: &EveSessionId) -> Result<(), EveError> {
        let path = format!("/eve/v1/session/{session}/cancel");
        self.post(assertion, &path, json!({})).await.map(drop)
    }

    /// Reads stored lines from `start` until eve ends the lease (the caller reconnects from its
    /// cursor) or the session ends.
    pub async fn stream(
        &self,
        assertion: &str,
        session: &EveSessionId,
        start: u64,
    ) -> Result<EveStream, EveError> {
        let mut url = self.url(&format!("/eve/v1/session/{session}/stream"))?;
        url.query_pairs_mut()
            .append_pair("startIndex", &start.to_string());
        let res = self
            .http
            .get(url)
            .bearer_auth(assertion)
            .send()
            .await
            .map_err(EveError::Request)?;
        if !res.status().is_success() {
            return Err(EveError::Status(res.status().as_u16()));
        }
        Ok(EveStream {
            res,
            buf: Vec::new(),
            next: start,
            done: false,
        })
    }
}

/// An NDJSON stream of stored lines; transport records (heartbeats, lease ends) are not lines.
pub struct EveStream {
    res: reqwest::Response,
    buf: Vec<u8>,
    next: u64,
    done: bool,
}

#[derive(Deserialize)]
struct RawLine {
    #[serde(rename = "$eve")]
    transport: Option<String>,
    facts: Option<Vec<Fact>>,
    progress: Option<Value>,
}

impl EveStream {
    /// The next record, or `None` when this read ends.
    pub async fn next(&mut self) -> Result<Option<Record>, EveError> {
        while !self.done {
            if let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                if let Some(r) = self.parse(&line[..line.len() - 1])? {
                    return Ok(Some(r));
                }
                continue;
            }
            match self.res.chunk().await.map_err(EveError::Request)? {
                Some(c) => {
                    self.buf.extend_from_slice(&c);
                    if self.buf.len() > MAX_LINE_BYTES {
                        return Err(EveError::Malformed);
                    }
                }
                None => self.done = true,
            }
        }
        Ok(None)
    }

    fn parse(&mut self, line: &[u8]) -> Result<Option<Record>, EveError> {
        if line.iter().all(u8::is_ascii_whitespace) {
            return Ok(None);
        }
        let raw: RawLine = serde_json::from_slice(line).map_err(|_| EveError::Malformed)?;
        match (raw.transport.as_deref(), raw.facts, raw.progress) {
            (Some("stream.ended"), ..) => {
                self.done = true;
                Ok(Some(Record::Ended))
            }
            (Some("stream.lease-ended"), ..) => {
                self.done = true;
                Ok(None)
            }
            // Heartbeats and transport records a later eve adds are not stored lines.
            (Some(_), ..) => Ok(None),
            (None, Some(facts), _) => Ok(Some(self.line(facts))),
            (None, None, Some(_)) => Ok(Some(self.line(vec![]))),
            (None, None, None) => Err(EveError::Malformed),
        }
    }

    fn line(&mut self, facts: Vec<Fact>) -> Record {
        let index = self.next;
        self.next += 1;
        Record::Line { index, facts }
    }
}
