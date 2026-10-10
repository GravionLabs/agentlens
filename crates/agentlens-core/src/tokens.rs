//! Token counts for content that no client reports a count for (ADR 0001).

use serde::{Deserialize, Serialize};

/// A token count and whether it was measured by a client or estimated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub count: u64,
    pub measured: bool,
}

impl Tokens {
    pub fn estimated(count: u64) -> Self {
        Tokens {
            count,
            measured: false,
        }
    }

    pub fn measured(count: u64) -> Self {
        Tokens {
            count,
            measured: true,
        }
    }
}

/// Counts the tokens of a text.
pub trait TokenEstimator {
    fn count(&self, text: &str) -> Tokens;
}

/// `characters / 4`, rounded up. Offline, deterministic, always an estimate.
#[derive(Debug, Clone, Copy, Default)]
pub struct CharsPerToken;

impl TokenEstimator for CharsPerToken {
    fn count(&self, text: &str) -> Tokens {
        Tokens::estimated((text.chars().count() as u64).div_ceil(4))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_up_and_counts_characters_not_bytes() {
        let e = CharsPerToken;
        assert_eq!(e.count("").count, 0);
        assert_eq!(e.count("abcd").count, 1);
        assert_eq!(e.count("abcde").count, 2);
        assert_eq!(e.count("äöüß").count, 1, "four characters, eight bytes");
    }

    #[test]
    fn estimates_are_never_marked_measured() {
        assert!(!CharsPerToken.count("hello").measured);
        assert!(Tokens::measured(3).measured);
    }
}
