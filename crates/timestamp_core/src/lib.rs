//! Unix timestamp and date/time conversion engine, ported from Swift `TimestampCore`.

use chrono::{DateTime, FixedOffset, Local, NaiveDate, TimeZone as ChronoTimeZone};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampUnit {
    Automatic,
    Seconds,
    Milliseconds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampInputKind {
    Timestamp { unit: TimestampUnit },
    DateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampAnalysis {
    pub input: String,
    pub input_kind: Option<TimestampInputKind>,
    pub instant: Option<i64>,
    pub local_text: Option<String>,
    pub utc_text: Option<String>,
    pub seconds_text: Option<String>,
    pub milliseconds_text: Option<String>,
    pub diagnostic: Option<String>,
}

/// Interpretation / formatting zone. `Local` is the system zone; tests use fixed offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeZone {
    Local,
    Fixed { offset_seconds: i32 },
}

impl TimeZone {
    pub const UTC: Self = Self::Fixed { offset_seconds: 0 };

    pub fn gmt_plus_08() -> Self {
        Self::Fixed {
            offset_seconds: 8 * 60 * 60,
        }
    }
}

pub struct TimestampEngine;

impl Default for TimestampEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TimestampEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(
        &self,
        input: &str,
        unit: TimestampUnit,
        time_zone: TimeZone,
    ) -> TimestampAnalysis {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Self::failure(input, "Enter a Unix timestamp or date/time.");
        }

        if let Some(analysis) = self.analyze_numeric(trimmed, input, unit, time_zone) {
            return analysis;
        }

        if let Some(millis) = Self::parse_date(trimmed, time_zone) {
            return Self::make_date_analysis(input, millis, time_zone);
        }

        Self::failure(
            input,
            "Unsupported date format. Use ISO 8601 or yyyy-MM-dd HH:mm:ss.",
        )
    }

    pub fn format_date(milliseconds: i64, time_zone: TimeZone) -> Option<String> {
        let utc = DateTime::from_timestamp_millis(milliseconds)?;
        let (naive, offset) = match time_zone {
            TimeZone::Fixed { offset_seconds } => {
                let offset_tz = FixedOffset::east_opt(offset_seconds)?;
                let dt = utc.with_timezone(&offset_tz);
                (dt.naive_local(), offset_seconds)
            }
            TimeZone::Local => {
                let dt = utc.with_timezone(&Local);
                (dt.naive_local(), dt.offset().local_minus_utc())
            }
        };
        Some(format!(
            "{} {}",
            naive.format("%Y-%m-%d %H:%M:%S"),
            time_zone_label(offset)
        ))
    }

    pub fn time_zone_label_at(milliseconds: i64, time_zone: TimeZone) -> String {
        let offset = match time_zone {
            TimeZone::Fixed { offset_seconds } => offset_seconds,
            TimeZone::Local => DateTime::from_timestamp_millis(milliseconds)
                .map(|utc| utc.with_timezone(&Local).offset().local_minus_utc())
                .unwrap_or(0),
        };
        time_zone_label(offset)
    }

    fn analyze_numeric(
        &self,
        trimmed: &str,
        original: &str,
        unit: TimestampUnit,
        time_zone: TimeZone,
    ) -> Option<TimestampAnalysis> {
        if !is_anchored_numeric(trimmed) {
            return None;
        }

        let classification = classify_numeric_input(trimmed, unit);
        if classification == TimestampUnit::Seconds {
            let Some(milliseconds) = milliseconds_from_seconds_string(trimmed) else {
                return Some(Self::failure(original, "Timestamp value is out of range."));
            };
            if !instant_is_representable(milliseconds) {
                return Some(Self::failure(original, "Timestamp value is out of range."));
            }
            Some(Self::make_timestamp_analysis(
                original,
                milliseconds,
                TimestampUnit::Seconds,
                time_zone,
            ))
        } else {
            if trimmed.contains('.') {
                return Some(Self::failure(
                    original,
                    "Fractional milliseconds are not supported.",
                ));
            }
            let Some(milliseconds) = integer_milliseconds(trimmed) else {
                return Some(Self::failure(original, "Timestamp value is out of range."));
            };
            if !instant_is_representable(milliseconds) {
                return Some(Self::failure(original, "Timestamp value is out of range."));
            }
            Some(Self::make_timestamp_analysis(
                original,
                milliseconds,
                TimestampUnit::Milliseconds,
                time_zone,
            ))
        }
    }

    fn make_timestamp_analysis(
        original: &str,
        milliseconds: i64,
        interpreted_unit: TimestampUnit,
        time_zone: TimeZone,
    ) -> TimestampAnalysis {
        TimestampAnalysis {
            input: original.to_string(),
            input_kind: Some(TimestampInputKind::Timestamp {
                unit: interpreted_unit,
            }),
            instant: Some(milliseconds),
            local_text: Self::format_date(milliseconds, time_zone),
            utc_text: Self::format_date(milliseconds, TimeZone::UTC),
            seconds_text: Some(formatted_seconds_text(milliseconds)),
            milliseconds_text: Some(milliseconds.to_string()),
            diagnostic: None,
        }
    }

    fn make_date_analysis(
        original: &str,
        milliseconds: i64,
        time_zone: TimeZone,
    ) -> TimestampAnalysis {
        TimestampAnalysis {
            input: original.to_string(),
            input_kind: Some(TimestampInputKind::DateTime),
            instant: Some(milliseconds),
            local_text: Self::format_date(milliseconds, time_zone),
            utc_text: Self::format_date(milliseconds, TimeZone::UTC),
            seconds_text: Some(formatted_seconds_text(milliseconds)),
            milliseconds_text: Some(milliseconds.to_string()),
            diagnostic: None,
        }
    }

    fn parse_date(trimmed: &str, time_zone: TimeZone) -> Option<i64> {
        if let Some(millis) = parse_explicit_offset_date(trimmed) {
            return Some(millis);
        }
        parse_naive_date(trimmed, time_zone)
    }

    fn failure(input: &str, diagnostic: &str) -> TimestampAnalysis {
        TimestampAnalysis {
            input: input.to_string(),
            input_kind: None,
            instant: None,
            local_text: None,
            utc_text: None,
            seconds_text: None,
            milliseconds_text: None,
            diagnostic: Some(diagnostic.to_string()),
        }
    }
}

fn time_zone_label(offset_seconds: i32) -> String {
    if offset_seconds == 0 {
        return "UTC".to_string();
    }
    let sign = if offset_seconds >= 0 { '+' } else { '-' };
    let magnitude = offset_seconds.unsigned_abs();
    let hours = magnitude / 3600;
    let minutes = (magnitude % 3600) / 60;
    format!("GMT{sign}{hours:02}:{minutes:02}")
}

fn instant_is_representable(milliseconds: i64) -> bool {
    DateTime::from_timestamp_millis(milliseconds).is_some()
}

fn is_anchored_numeric(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let rest = if first == '+' || first == '-' {
        chars.as_str()
    } else {
        input
    };
    if rest.is_empty() {
        return false;
    }
    let mut saw_digit = false;
    let mut saw_dot = false;
    let mut saw_frac = false;
    for c in rest.chars() {
        if c.is_ascii_digit() {
            if saw_dot {
                saw_frac = true;
            } else {
                saw_digit = true;
            }
        } else if c == '.' && !saw_dot {
            saw_dot = true;
        } else {
            return false;
        }
    }
    saw_digit && (!saw_dot || saw_frac)
}

fn classify_numeric_input(trimmed: &str, unit: TimestampUnit) -> TimestampUnit {
    match unit {
        TimestampUnit::Seconds | TimestampUnit::Milliseconds => unit,
        TimestampUnit::Automatic => {
            if trimmed.contains('.') {
                TimestampUnit::Seconds
            } else if absolute_integer_magnitude_is_at_least_threshold(trimmed) {
                TimestampUnit::Milliseconds
            } else {
                TimestampUnit::Seconds
            }
        }
    }
}

fn absolute_integer_magnitude_is_at_least_threshold(input: &str) -> bool {
    let mut normalized = input;
    if normalized.starts_with('+') || normalized.starts_with('-') {
        normalized = &normalized[1..];
    }
    let normalized = normalized.trim_start_matches('0');
    let normalized = if normalized.is_empty() {
        "0"
    } else {
        normalized
    };
    let threshold = "100000000000";
    if normalized.len() != threshold.len() {
        normalized.len() > threshold.len()
    } else {
        normalized >= threshold
    }
}

fn milliseconds_from_seconds_string(input: &str) -> Option<i64> {
    let (negative, int_part, frac_part) = split_signed_decimal(input)?;
    let mut seconds = parse_i64_digits(int_part)?;
    let mut frac_ms = fraction_to_millis(frac_part)?;
    if frac_ms >= 1000 {
        seconds = seconds.checked_add(1)?;
        frac_ms = 0;
    }
    let mut ms = seconds.checked_mul(1000)?.checked_add(i64::from(frac_ms))?;
    if negative {
        ms = ms.checked_neg()?;
    }
    Some(ms)
}

fn integer_milliseconds(input: &str) -> Option<i64> {
    let (negative, int_part, frac_part) = split_signed_decimal(input)?;
    if !frac_part.is_empty() {
        return None;
    }
    let mut value = parse_i64_digits(int_part)?;
    if negative {
        value = value.checked_neg()?;
    }
    Some(value)
}

fn split_signed_decimal(input: &str) -> Option<(bool, &str, &str)> {
    let (negative, rest) = if let Some(rest) = input.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = input.strip_prefix('+') {
        (false, rest)
    } else {
        (false, input)
    };
    let (int_part, frac_part) = match rest.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (rest, ""),
    };
    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !frac_part.is_empty() && !frac_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((negative, int_part, frac_part))
}

fn parse_i64_digits(digits: &str) -> Option<i64> {
    digits.parse::<i64>().ok()
}

fn fraction_to_millis(frac: &str) -> Option<u32> {
    if frac.is_empty() {
        return Some(0);
    }
    if !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if frac.len() <= 3 {
        let mut padded = frac.to_string();
        while padded.len() < 3 {
            padded.push('0');
        }
        return padded.parse().ok();
    }
    let whole: u32 = frac[..3].parse().ok()?;
    let next = frac.as_bytes()[3] - b'0';
    if next >= 5 {
        Some(whole + 1)
    } else {
        Some(whole)
    }
}

fn formatted_seconds_text(milliseconds: i64) -> String {
    let negative = milliseconds < 0;
    let magnitude = milliseconds.unsigned_abs();
    let whole = magnitude / 1000;
    let frac = magnitude % 1000;
    if frac == 0 {
        if negative {
            format!("-{whole}")
        } else {
            format!("{whole}")
        }
    } else {
        let frac_str = format!("{frac:03}");
        let frac_str = frac_str.trim_end_matches('0');
        if negative {
            format!("-{whole}.{frac_str}")
        } else {
            format!("{whole}.{frac_str}")
        }
    }
}

fn parse_explicit_offset_date(input: &str) -> Option<i64> {
    let normalized = normalize_zulu_or_offset(input);
    let t = normalized.find('T')?;
    let (year, month, day) = parse_ymd(&normalized[..t])?;
    let rest = &normalized[t + 1..];

    let (time_part, offset_seconds) = if let Some(rest_z) = rest.strip_suffix('Z') {
        (rest_z, 0i32)
    } else {
        let sign_idx = rest.rfind('+').or_else(|| rest.rfind('-'))?;
        if sign_idx == 0 {
            return None;
        }
        let offset = parse_offset(&rest[sign_idx..])?;
        (&rest[..sign_idx], offset)
    };

    let (hour, minute, second, millis) = parse_hms_optional_frac(time_part)?;
    naive_and_offset_to_millis(
        year,
        month,
        day,
        hour,
        minute,
        second,
        millis,
        offset_seconds,
    )
}

fn normalize_zulu_or_offset(input: &str) -> String {
    let bytes = input.as_bytes();
    if bytes.len() >= 5 {
        let start = bytes.len() - 5;
        let sign = bytes[start];
        if (sign == b'+' || sign == b'-') && bytes[start + 1..].iter().all(|b| b.is_ascii_digit()) {
            return format!(
                "{}{}:{}",
                &input[..start],
                &input[start..start + 3],
                &input[start + 3..]
            );
        }
    }
    input.to_string()
}

fn parse_offset(offset: &str) -> Option<i32> {
    if offset.len() != 6 {
        return None;
    }
    let bytes = offset.as_bytes();
    if (bytes[0] != b'+' && bytes[0] != b'-') || bytes[3] != b':' {
        return None;
    }
    let hours: i32 = offset[1..3].parse().ok()?;
    let minutes: i32 = offset[4..6].parse().ok()?;
    if !(0..=23).contains(&hours) || minutes > 59 {
        return None;
    }
    let magnitude = hours * 3600 + minutes * 60;
    Some(if bytes[0] == b'-' {
        -magnitude
    } else {
        magnitude
    })
}

fn parse_naive_date(input: &str, time_zone: TimeZone) -> Option<i64> {
    let (year, month, day, hour, minute, second, millis) = parse_naive_components(input)?;
    match time_zone {
        TimeZone::Fixed { offset_seconds } => naive_and_offset_to_millis(
            year,
            month,
            day,
            hour,
            minute,
            second,
            millis,
            offset_seconds,
        ),
        TimeZone::Local => {
            let naive = NaiveDate::from_ymd_opt(year, month, day)?
                .and_hms_milli_opt(hour, minute, second, millis)?;
            let dt = Local
                .from_local_datetime(&naive)
                .single()
                .or_else(|| Local.from_local_datetime(&naive).earliest())?;
            Some(dt.timestamp_millis())
        }
    }
}

fn parse_naive_components(input: &str) -> Option<(i32, u32, u32, u32, u32, u32, u32)> {
    const PATTERNS: [(&str, bool, usize); 4] = [
        ("T", true, 23),
        ("T", false, 19),
        (" ", true, 23),
        (" ", false, 19),
    ];
    for &(sep, with_ms, len) in &PATTERNS {
        if input.len() != len {
            continue;
        }
        if let Some(parsed) = parse_fixed_naive(input, sep, with_ms) {
            return Some(parsed);
        }
    }
    None
}

fn parse_fixed_naive(
    input: &str,
    sep: &str,
    with_ms: bool,
) -> Option<(i32, u32, u32, u32, u32, u32, u32)> {
    let (date, time) = input.split_once(sep)?;
    let (year, month, day) = parse_ymd(date)?;
    if with_ms {
        let (hms, ms) = time.split_once('.')?;
        if ms.len() != 3 || !ms.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (hour, minute, second) = parse_hms(hms)?;
        let millis: u32 = ms.parse().ok()?;
        Some((year, month, day, hour, minute, second, millis))
    } else {
        if time.contains('.') {
            return None;
        }
        let (hour, minute, second) = parse_hms(time)?;
        Some((year, month, day, hour, minute, second, 0))
    }
}

fn parse_ymd(date: &str) -> Option<(i32, u32, u32)> {
    if date.len() != 10
        || date.as_bytes().get(4) != Some(&b'-')
        || date.as_bytes().get(7) != Some(&b'-')
    {
        return None;
    }
    if !date.as_bytes()[0..4]
        .iter()
        .chain(&date.as_bytes()[5..7])
        .chain(&date.as_bytes()[8..10])
        .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let year: i32 = date[0..4].parse().ok()?;
    let month: u32 = date[5..7].parse().ok()?;
    let day: u32 = date[8..10].parse().ok()?;
    let max_day = days_in_month(year, month)?;
    if day < 1 || day > max_day {
        return None;
    }
    Some((year, month, day))
}

fn days_in_month(year: i32, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if is_leap(year) { 29 } else { 28 }),
        _ => None,
    }
}

fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn parse_hms(time: &str) -> Option<(u32, u32, u32)> {
    if time.len() != 8
        || time.as_bytes().get(2) != Some(&b':')
        || time.as_bytes().get(5) != Some(&b':')
    {
        return None;
    }
    if !time.as_bytes()[0..2]
        .iter()
        .chain(&time.as_bytes()[3..5])
        .chain(&time.as_bytes()[6..8])
        .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let hour: u32 = time[0..2].parse().ok()?;
    let minute: u32 = time[3..5].parse().ok()?;
    let second: u32 = time[6..8].parse().ok()?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some((hour, minute, second))
}

fn parse_hms_optional_frac(time: &str) -> Option<(u32, u32, u32, u32)> {
    if let Some((hms, frac)) = time.split_once('.') {
        if frac.is_empty() {
            return None;
        }
        let (mut hour, mut minute, mut second) = parse_hms(hms)?;
        let mut millis = fraction_to_millis(frac)?;
        if millis >= 1000 {
            millis = 0;
            second += 1;
            if second >= 60 {
                second = 0;
                minute += 1;
                if minute >= 60 {
                    minute = 0;
                    hour += 1;
                    if hour >= 24 {
                        return None;
                    }
                }
            }
        }
        Some((hour, minute, second, millis))
    } else {
        let (hour, minute, second) = parse_hms(time)?;
        Some((hour, minute, second, 0))
    }
}

fn naive_and_offset_to_millis(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millis: u32,
    offset_seconds: i32,
) -> Option<i64> {
    let naive = NaiveDate::from_ymd_opt(year, month, day)?
        .and_hms_milli_opt(hour, minute, second, millis)?;
    let offset = FixedOffset::east_opt(offset_seconds)?;
    let dt = offset.from_local_datetime(&naive).single()?;
    Some(dt.timestamp_millis())
}
