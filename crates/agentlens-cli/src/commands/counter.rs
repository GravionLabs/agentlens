use agentlens_core::env::Env;
use agentlens_core::exact::{DEFAULT_MODEL, ExactTokens};
use agentlens_core::tokens::{CharsPerToken, TokenEstimator};
use anyhow::{Result, bail};

/// The token counter of a run: the offline estimate, or the API with `--exact`.
pub struct Counter {
    exact: Option<ExactTokens>,
}

impl Counter {
    /// `--exact` stops with a clear message without `ANTHROPIC_API_KEY`.
    pub fn new(env: &Env, exact: bool, model: Option<&str>) -> Result<Counter> {
        if !exact {
            return Ok(Counter { exact: None });
        }
        let key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        if key.trim().is_empty() {
            bail!(
                "--exact needs the ANTHROPIC_API_KEY environment variable; \
                 without --exact nothing is sent anywhere"
            );
        }
        let model = model.unwrap_or(DEFAULT_MODEL);
        // The variable the Anthropic SDKs use for a proxy or gateway; tests point it at a mock.
        let exact = match std::env::var("ANTHROPIC_BASE_URL")
            .ok()
            .filter(|u| !u.is_empty())
        {
            Some(url) => ExactTokens::with(
                key.trim().to_string(),
                model.to_string(),
                url,
                Some(ExactTokens::cache_path(env, model)),
            ),
            None => ExactTokens::new(env, key.trim(), model),
        };
        Ok(Counter { exact: Some(exact) })
    }

    pub fn estimator(&self) -> &dyn TokenEstimator {
        match &self.exact {
            Some(e) => e,
            None => &CharsPerToken,
        }
    }

    /// Saves the cache and tells about problems on stderr.
    pub fn finish(&self) {
        if let Some(e) = &self.exact {
            e.save();
            for w in e.warnings() {
                eprintln!("warning: {w}");
            }
        }
    }
}
