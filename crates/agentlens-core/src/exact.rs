//! Exact token counts through the Anthropic `count_tokens` endpoint (ADR 0001, option 3).
//!
//! Opt-in: the text of skills is sent to the API only when an [`ExactTokens`]
//! is created. Results are cached on disk by model and content hash. Any
//! failure falls back to the `characters / 4` estimate with one warning and
//! stops further requests; it never aborts a scan.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::env::Env;
use crate::tokens::{CharsPerToken, TokenEstimator, Tokens};

/// The model whose tokenizer counts, unless another one is chosen.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";
const CACHE_VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    counts: BTreeMap<String, u64>,
}

#[derive(Debug, Default)]
struct State {
    counts: BTreeMap<String, u64>,
    dirty: bool,
    /// Set after the first failed request: the rest is estimated without asking again.
    disabled: bool,
    warnings: Vec<String>,
    api_calls: u64,
}

/// Counts with the API and caches the result; see the module documentation.
pub struct ExactTokens {
    api_key: String,
    model: String,
    base_url: String,
    cache_file: Option<PathBuf>,
    agent: ureq::Agent,
    state: Mutex<State>,
}

impl ExactTokens {
    /// `<cache home>/agentlens/count_tokens/<model>.json`
    pub fn cache_path(env: &Env, model: &str) -> PathBuf {
        let name: String = model
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        env.cache_home()
            .join("agentlens/count_tokens")
            .join(format!("{name}.json"))
    }

    /// A counter for `model`, caching in the cache directory of `env`.
    pub fn new(env: &Env, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        let file = Self::cache_path(env, &model);
        Self::with(
            api_key.into(),
            model,
            DEFAULT_BASE_URL.to_string(),
            Some(file),
        )
    }

    /// A counter that talks to another server (tests) and optionally caches in `cache_file`.
    pub fn with(
        api_key: String,
        model: String,
        base_url: String,
        cache_file: Option<PathBuf>,
    ) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .build()
            .into();
        let counts = cache_file.as_deref().map(read_cache).unwrap_or_default();
        ExactTokens {
            api_key,
            model,
            base_url: base_url.trim_end_matches('/').to_string(),
            cache_file,
            agent,
            state: Mutex::new(State {
                counts,
                ..State::default()
            }),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Requests sent to the API so far (cache hits do not count).
    pub fn api_calls(&self) -> u64 {
        self.state().api_calls
    }

    /// What went wrong, at most one message per run.
    pub fn warnings(&self) -> Vec<String> {
        self.state().warnings.clone()
    }

    /// Writes new counts to the cache file. Failure is a warning, not an error.
    pub fn save(&self) {
        let mut st = self.state();
        if !st.dirty {
            return;
        }
        let Some(path) = &self.cache_file else {
            return;
        };
        let file = CacheFile {
            version: CACHE_VERSION,
            counts: st.counts.clone(),
        };
        match write_cache(path, &file) {
            Ok(()) => st.dirty = false,
            Err(e) => st
                .warnings
                .push(format!("could not write {}: {e}", path.display())),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn key(text: &str) -> String {
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    fn request(&self, text: &str) -> Result<u64, String> {
        let url = format!("{}/v1/messages/count_tokens", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": text }],
        });
        let mut response = self
            .agent
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .send_json(&body)
            .map_err(|e| format!("count_tokens request failed: {e}"))?;
        let status = response.status().as_u16();
        if status != 200 {
            let hint = match status {
                401 | 403 => " (the API key was rejected)",
                429 => " (rate limited)",
                _ => "",
            };
            return Err(format!("count_tokens answered HTTP {status}{hint}"));
        }
        let value: serde_json::Value = response
            .body_mut()
            .read_json()
            .map_err(|e| format!("count_tokens answered something unexpected: {e}"))?;
        value
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| "count_tokens answered without input_tokens".to_string())
    }
}

impl TokenEstimator for ExactTokens {
    fn count(&self, text: &str) -> Tokens {
        if text.is_empty() {
            return Tokens::measured(0);
        }
        let key = Self::key(text);
        {
            let st = self.state();
            if let Some(n) = st.counts.get(&key) {
                return Tokens::measured(*n);
            }
            if st.disabled {
                return CharsPerToken.count(text);
            }
        }
        // The lock is not held during the request; counting is sequential anyway.
        let result = self.request(text);
        let mut st = self.state();
        st.api_calls += 1;
        match result {
            Ok(n) => {
                st.counts.insert(key, n);
                st.dirty = true;
                Tokens::measured(n)
            }
            Err(message) => {
                st.disabled = true;
                st.warnings
                    .push(format!("{message}; the remaining counts are estimated"));
                CharsPerToken.count(text)
            }
        }
    }

    fn count_bytes(&self, bytes: u64) -> Tokens {
        // Only the size is known, the text is not read: always an estimate.
        CharsPerToken.count_bytes(bytes)
    }
}

impl Drop for ExactTokens {
    fn drop(&mut self) {
        self.save();
    }
}

fn read_cache(path: &Path) -> BTreeMap<String, u64> {
    let Ok(text) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    match serde_json::from_str::<CacheFile>(&text) {
        Ok(f) if f.version == CACHE_VERSION => f.counts,
        _ => BTreeMap::new(),
    }
}

fn write_cache(path: &Path, file: &CacheFile) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec(file)?)?;
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Answers every request with `status` and `body`; counts the requests.
    struct Mock {
        url: String,
        hits: Arc<AtomicU64>,
    }

    fn mock(status: u16, body: &'static str) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicU64::new(0));
        let counter = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                let mut buf = vec![0u8; 8192];
                let mut seen = Vec::new();
                // Read until the end of the headers plus the announced body.
                loop {
                    let n = s.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    seen.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&seen).to_string();
                    if let Some(h) = text.find("\r\n\r\n") {
                        let len = text[..h]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if seen.len() >= h + 4 + len {
                            break;
                        }
                    }
                }
                counter.fetch_add(1, Ordering::SeqCst);
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(reply.as_bytes());
            }
        });
        Mock { url, hits }
    }

    fn exact(url: &str, cache: Option<PathBuf>) -> ExactTokens {
        ExactTokens::with("key".into(), "m".into(), url.into(), cache)
    }

    #[test]
    fn counts_with_the_api_and_marks_the_result_measured() {
        let m = mock(200, r#"{"input_tokens": 42}"#);
        let e = exact(&m.url, None);
        assert_eq!(e.count("hello"), Tokens::measured(42));
        assert!(e.warnings().is_empty());
    }

    #[test]
    fn empty_text_costs_nothing_and_asks_nobody() {
        let m = mock(200, r#"{"input_tokens": 9}"#);
        let e = exact(&m.url, None);
        assert_eq!(e.count(""), Tokens::measured(0));
        assert_eq!(m.hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn repeated_text_and_later_runs_use_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("c/m.json");
        let m = mock(200, r#"{"input_tokens": 7}"#);
        {
            let e = exact(&m.url, Some(file.clone()));
            assert_eq!(e.count("same").count, 7);
            assert_eq!(e.count("same").count, 7);
            assert_eq!(e.api_calls(), 1);
            e.save();
        }
        let again = exact(&m.url, Some(file));
        assert_eq!(again.count("same"), Tokens::measured(7));
        assert_eq!(again.api_calls(), 0);
        assert_eq!(m.hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_rejected_key_falls_back_once_with_a_warning() {
        let m = mock(401, r#"{"error":{"message":"invalid x-api-key"}}"#);
        let e = exact(&m.url, None);
        let a = e.count("abcdefgh");
        let b = e.count("another text");
        assert_eq!(a, Tokens::estimated(2));
        assert!(!b.measured);
        assert_eq!(m.hits.load(Ordering::SeqCst), 1, "no further requests");
        let w = e.warnings();
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("401") && w[0].contains("rejected"), "{w:?}");
    }

    #[test]
    fn an_unreachable_server_never_aborts() {
        let e = exact("http://127.0.0.1:1", None);
        assert_eq!(e.count("abcd"), Tokens::estimated(1));
        assert_eq!(e.warnings().len(), 1);
    }

    #[test]
    fn an_answer_without_a_count_is_a_failure() {
        let m = mock(200, r#"{"other": 1}"#);
        let e = exact(&m.url, None);
        assert!(!e.count("abcd").measured);
        assert!(e.warnings()[0].contains("input_tokens"));
    }

    #[test]
    fn sizes_alone_stay_estimates() {
        let e = exact("http://127.0.0.1:1", None);
        assert_eq!(e.count_bytes(8), Tokens::estimated(2));
    }

    #[test]
    fn a_broken_cache_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("m.json");
        fs::write(&file, "not json").unwrap();
        let m = mock(200, r#"{"input_tokens": 3}"#);
        assert_eq!(exact(&m.url, Some(file)).count("x").count, 3);
    }

    #[test]
    fn the_cache_path_is_per_model_and_safe() {
        let env = Env::for_home("/h");
        assert_eq!(
            ExactTokens::cache_path(&env, "claude/x y"),
            PathBuf::from("/h/.cache/agentlens/count_tokens/claude_x_y.json")
        );
    }
}
