use timestamp_core::{TimeZone, TimestampEngine, TimestampInputKind, TimestampUnit};

fn local() -> TimeZone {
    TimeZone::gmt_plus_08()
}

fn utc() -> TimeZone {
    TimeZone::UTC
}

#[test]
fn automatic_seconds_and_milliseconds_represent_the_same_instant() {
    let engine = TimestampEngine::new();
    let seconds = engine.analyze("1786500000", TimestampUnit::Automatic, local());
    let millis = engine.analyze("1786500000000", TimestampUnit::Automatic, local());
    assert_eq!(
        seconds.input_kind,
        Some(TimestampInputKind::Timestamp {
            unit: TimestampUnit::Seconds
        })
    );
    assert_eq!(
        millis.input_kind,
        Some(TimestampInputKind::Timestamp {
            unit: TimestampUnit::Milliseconds
        })
    );
    assert_eq!(seconds.instant, millis.instant);
    assert_eq!(seconds.milliseconds_text.as_deref(), Some("1786500000000"));
}

#[test]
fn fractional_seconds_produce_millisecond_precision() {
    let result = TimestampEngine::new().analyze("1700000000.125", TimestampUnit::Automatic, utc());
    assert_eq!(
        result.input_kind,
        Some(TimestampInputKind::Timestamp {
            unit: TimestampUnit::Seconds
        })
    );
    assert_eq!(result.seconds_text.as_deref(), Some("1700000000.125"));
    assert_eq!(result.milliseconds_text.as_deref(), Some("1700000000125"));
}

#[test]
fn manual_unit_overrides_automatic_detection() {
    let result = TimestampEngine::new().analyze("1700000000", TimestampUnit::Milliseconds, utc());
    assert_eq!(
        result.input_kind,
        Some(TimestampInputKind::Timestamp {
            unit: TimestampUnit::Milliseconds
        })
    );
    assert_eq!(result.seconds_text.as_deref(), Some("1700000"));
}

#[test]
fn explicit_offset_and_utc_produce_the_same_instant() {
    let engine = TimestampEngine::new();
    let offset = engine.analyze("2026-08-12T15:30:00+08:00", TimestampUnit::Automatic, utc());
    let zulu = engine.analyze("2026-08-12T07:30:00Z", TimestampUnit::Automatic, local());
    assert_eq!(offset.input_kind, Some(TimestampInputKind::DateTime));
    assert_eq!(offset.instant, zulu.instant);
    assert_eq!(offset.utc_text.as_deref(), Some("2026-08-12 07:30:00 UTC"));
}

#[test]
fn timezone_less_date_uses_selected_timezone() {
    let result =
        TimestampEngine::new().analyze("2026-08-12 15:30:00", TimestampUnit::Automatic, local());
    assert_eq!(result.input_kind, Some(TimestampInputKind::DateTime));
    assert_eq!(
        result.local_text.as_deref(),
        Some("2026-08-12 15:30:00 GMT+08:00")
    );
    assert_eq!(result.utc_text.as_deref(), Some("2026-08-12 07:30:00 UTC"));
}

#[test]
fn invalid_input_returns_diagnostic_without_partial_output() {
    let empty = TimestampEngine::new().analyze("  ", TimestampUnit::Automatic, utc());
    let invalid = TimestampEngine::new().analyze("2026/08/12", TimestampUnit::Automatic, utc());
    assert_eq!(
        empty.diagnostic.as_deref(),
        Some("Enter a Unix timestamp or date/time.")
    );
    assert_eq!(empty.instant, None);
    assert_eq!(
        invalid.diagnostic.as_deref(),
        Some("Unsupported date format. Use ISO 8601 or yyyy-MM-dd HH:mm:ss.")
    );
    assert_eq!(invalid.local_text, None);
}

#[test]
fn fractional_milliseconds_are_rejected() {
    let result =
        TimestampEngine::new().analyze("1700000000000.5", TimestampUnit::Milliseconds, utc());
    assert_eq!(
        result.diagnostic.as_deref(),
        Some("Fractional milliseconds are not supported.")
    );
    assert_eq!(result.instant, None);
    assert_eq!(result.seconds_text, None);
    assert_eq!(result.milliseconds_text, None);
}

#[test]
fn manual_seconds_override_keeps_large_integer_as_seconds() {
    let result = TimestampEngine::new().analyze("1786500000000", TimestampUnit::Seconds, utc());
    assert_eq!(
        result.input_kind,
        Some(TimestampInputKind::Timestamp {
            unit: TimestampUnit::Seconds
        })
    );
    assert_eq!(result.seconds_text.as_deref(), Some("1786500000000"));
    assert_eq!(
        result.milliseconds_text.as_deref(),
        Some("1786500000000000")
    );
}

#[test]
fn compact_offset_and_t_separated_naive_datetime_round_trip() {
    let engine = TimestampEngine::new();
    let compact = engine.analyze("2026-08-12T15:30:00+0800", TimestampUnit::Automatic, utc());
    let naive_t = engine.analyze("2026-08-12T15:30:00", TimestampUnit::Automatic, local());
    assert_eq!(compact.input_kind, Some(TimestampInputKind::DateTime));
    assert_eq!(naive_t.input_kind, Some(TimestampInputKind::DateTime));
    assert_eq!(compact.instant, naive_t.instant);
    assert_eq!(compact.utc_text.as_deref(), Some("2026-08-12 07:30:00 UTC"));
}

#[test]
fn seconds_and_milliseconds_round_trip_to_the_same_instant() {
    let engine = TimestampEngine::new();
    let first = engine.analyze("1700000000.125", TimestampUnit::Automatic, utc());
    let seconds = first.seconds_text.clone().expect("seconds");
    let millis = first.milliseconds_text.clone().expect("millis");
    let from_seconds = engine.analyze(&seconds, TimestampUnit::Automatic, utc());
    let from_millis = engine.analyze(&millis, TimestampUnit::Automatic, utc());
    assert_eq!(from_seconds.instant, first.instant);
    assert_eq!(from_millis.instant, first.instant);
    assert_eq!(from_seconds.milliseconds_text, first.milliseconds_text);
    assert_eq!(from_millis.seconds_text, first.seconds_text);
}
