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

/// The period `[since, until)` from the texts of the two bounds, each in the forms of [`parse_since`]
/// and counted from the same `now`. A blank text is no bound. `names` are the labels the errors carry
/// (`since` and `until`, or `--since` and `--until` for a command line).
pub fn parse_window(
    since: Option<&str>,
    until: Option<&str>,
    now: Timestamp,
    names: (&str, &str),
) -> Result<(Option<Timestamp>, Option<Timestamp>), String> {
    let bound = |text: Option<&str>, name: &str| match text.map(str::trim).filter(|s| !s.is_empty())
    {
        Some(s) => parse_since(s, now)
            .map(Some)
            .map_err(|e| format!("{name}: {e}")),
        None => Ok(None),
    };
    let since_at = bound(since, names.0)?;
    let until_at = bound(until, names.1)?;
    if let (Some(s), Some(u)) = (since_at, until_at)
        && u <= s
    {
        return Err(format!(
            "{} ({u}) must be later than {} ({s})",
            names.1, names.0
        ));
    }
    Ok((since_at, until_at))
}

/// The comparison window of a trend from `SINCE..UNTIL`, each bound in the forms of [`parse_since`]
/// and counted from the same `now`. `name` is the label the errors carry (`against`, `--against`).
pub fn parse_against(
    text: &str,
    now: Timestamp,
    name: &str,
) -> Result<(Timestamp, Timestamp), String> {
    let (since, until) = text.split_once("..").ok_or_else(|| {
        format!("{name}: expected SINCE..UNTIL, for example 2026-09-01..2026-10-01")
    })?;
    let (since, until) = parse_window(Some(since), Some(until), now, (name, name))?;
    match (since, until) {
        (Some(s), Some(u)) => Ok((s, u)),
        _ => Err(format!(
            "{name}: both SINCE and UNTIL are needed in SINCE..UNTIL"
        )),
    }
}

/// The start of a period of `days` days that ends at `until`, or now: the default window of the
/// unused view when no `since` is given.
pub fn window_start(
    days: u32,
    until: Option<Timestamp>,
    now: Timestamp,
) -> Result<Timestamp, String> {
    parse_since(&format!("{days}d"), until.unwrap_or(now))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-10-10T12:00:00Z".parse().unwrap()
    }

    #[test]
    fn against_takes_two_bounds() {
        let (s, u) = parse_against("2026-09-01..2026-10-01", now(), "against").unwrap();
        assert_eq!(s.to_string(), "2026-09-01T00:00:00Z");
        assert_eq!(u.to_string(), "2026-10-01T00:00:00Z");
        let (s, u) = parse_against("60d..30d", now(), "against").unwrap();
        assert!(s < u);
    }

    #[test]
    fn a_malformed_against_is_an_error_with_its_name() {
        for bad in [
            "",
            "2026-09-01",
            "..2026-10-01",
            "2026-09-01..",
            "x..y",
            "30d..60d",
        ] {
            let err = parse_against(bad, now(), "--against").unwrap_err();
            assert!(err.starts_with("--against"), "{bad}: {err}");
        }
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

    #[test]
    fn a_window_has_two_bounds_that_must_be_ordered() {
        let w = parse_window(Some("2d"), Some("1d"), now(), ("since", "until")).unwrap();
        assert_eq!(
            (w.0.unwrap().to_string(), w.1.unwrap().to_string()),
            ("2026-10-08T12:00:00Z".into(), "2026-10-09T12:00:00Z".into())
        );
        assert_eq!(
            parse_window(None, Some(" "), now(), ("since", "until")).unwrap(),
            (None, None)
        );
        let err = parse_window(
            Some("2026-10-02"),
            Some("2026-10-01"),
            now(),
            ("--since", "--until"),
        )
        .unwrap_err();
        assert!(
            err.starts_with("--until (") && err.contains("--since"),
            "{err}"
        );
    }

    #[test]
    fn a_window_error_names_the_bound() {
        let err = parse_window(Some("soon"), None, now(), ("--since", "--until")).unwrap_err();
        assert!(err.starts_with("--since: cannot parse"), "{err}");
        let err = parse_window(None, Some("soon"), now(), ("since", "until")).unwrap_err();
        assert!(err.starts_with("until: cannot parse"), "{err}");
    }

    #[test]
    fn the_default_window_ends_at_until_or_now() {
        let until: Timestamp = "2026-01-20T00:00:00Z".parse().unwrap();
        assert_eq!(
            window_start(30, Some(until), now()).unwrap().to_string(),
            "2025-12-21T00:00:00Z"
        );
        assert_eq!(
            window_start(30, None, now()).unwrap().to_string(),
            "2026-09-10T12:00:00Z"
        );
    }
}
