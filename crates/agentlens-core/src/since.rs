//! The "since" part of a session filter: a relative duration or a point in time.

use jiff::{Span, Timestamp, tz::TimeZone};

/// `30d`, `12h`, `4w` (relative to `now`), a date (`2026-10-01`, midnight UTC)
/// or an RFC 3339 instant.
pub fn parse_since(text: &str, now: Timestamp) -> Result<Timestamp, String> {
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, unit) = text.split_at(split);
    if !digits.is_empty() && matches!(unit, "h" | "d" | "w") {
        let n: i64 = digits
            .parse()
            .map_err(|_| "invalid number in the period".to_owned())?;
        let span = match unit {
            "h" => Span::new().hours(n),
            "d" => Span::new().days(n),
            _ => Span::new().weeks(n),
        };
        let zoned = now
            .to_zoned(TimeZone::UTC)
            .checked_sub(span)
            .map_err(|_| "the period is out of range".to_owned())?;
        return Ok(zoned.timestamp());
    }
    if let Ok(date) = text.parse::<jiff::civil::Date>() {
        let zoned = date
            .at(0, 0, 0, 0)
            .to_zoned(TimeZone::UTC)
            .map_err(|e| e.to_string())?;
        return Ok(zoned.timestamp());
    }
    if let Ok(ts) = text.parse::<Timestamp>() {
        return Ok(ts);
    }
    Err(format!(
        "cannot parse `{text}`: use a duration like 30d or a date like 2026-10-01"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-10-10T12:00:00Z".parse().unwrap()
    }

    #[test]
    fn durations_dates_and_instants() {
        assert_eq!(
            parse_since("30d", now()).unwrap().to_string(),
            "2026-09-10T12:00:00Z"
        );
        assert_eq!(
            parse_since("12h", now()).unwrap().to_string(),
            "2026-10-10T00:00:00Z"
        );
        assert_eq!(
            parse_since("2026-10-01", now()).unwrap().to_string(),
            "2026-10-01T00:00:00Z"
        );
        assert_eq!(
            parse_since("2026-10-01T08:30:00Z", now())
                .unwrap()
                .to_string(),
            "2026-10-01T08:30:00Z"
        );
    }

    #[test]
    fn garbage_is_rejected_with_a_hint() {
        assert!(parse_since("soon", now()).unwrap_err().contains("30d"));
        assert!(parse_since("d", now()).is_err());
    }
}
