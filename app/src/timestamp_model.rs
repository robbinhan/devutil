//! Timestamp card text. Conversion itself stays in `timestamp_core`.

use timestamp_core::{
    TimeZone, TimestampAnalysis, TimestampEngine, TimestampInputKind, TimestampUnit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayZone {
    Local,
    Utc,
}

impl DisplayZone {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Utc => "UTC",
        }
    }

    pub fn time_zone(self) -> TimeZone {
        match self {
            Self::Local => TimeZone::Local,
            Self::Utc => TimeZone::UTC,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultCard {
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: String,
    pub value: String,
}

/// Same four cards as the GPUI workbench. "Local time" is always the
/// system zone; the Local/UTC control only changes how a date without an
/// offset is interpreted.
pub fn result_cards(analysis: &TimestampAnalysis) -> Vec<ResultCard> {
    if analysis.diagnostic.is_some() {
        return Vec::new();
    }
    let mut cards = Vec::new();
    if let Some(instant) = analysis.instant {
        let value = TimestampEngine::format_date(instant, TimeZone::Local).unwrap_or_default();
        let label = TimestampEngine::time_zone_label_at(instant, TimeZone::Local);
        cards.push(ResultCard {
            id: "local-time",
            title: "Local time",
            subtitle: format!("This Mac ({label})"),
            value,
        });
    }
    if let Some(value) = &analysis.utc_text {
        cards.push(ResultCard {
            id: "utc",
            title: "UTC",
            subtitle: "Reference time".to_string(),
            value: value.clone(),
        });
    }
    if let Some(value) = &analysis.seconds_text {
        cards.push(ResultCard {
            id: "seconds",
            title: "Unix seconds",
            subtitle: "Unix epoch".to_string(),
            value: value.clone(),
        });
    }
    if let Some(value) = &analysis.milliseconds_text {
        cards.push(ResultCard {
            id: "milliseconds",
            title: "Unix milliseconds",
            subtitle: "Unix epoch".to_string(),
            value: value.clone(),
        });
    }
    cards
}

pub fn detected_description(analysis: &TimestampAnalysis, zone: DisplayZone) -> String {
    match analysis.input_kind {
        Some(TimestampInputKind::Timestamp { unit }) => {
            if unit == TimestampUnit::Milliseconds {
                "Unix milliseconds".to_string()
            } else {
                "Unix seconds".to_string()
            }
        }
        Some(TimestampInputKind::DateTime) => {
            format!(
                "date/time ({} interpretation when no offset is present)",
                zone.label()
            )
        }
        None => "unrecognized input".to_string(),
    }
}

#[allow(dead_code)]
pub fn char_label(count: usize) -> String {
    if count == 1 {
        "1 char".to_string()
    } else {
        format!("{count} chars")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Muted,
    Info,
    Success,
    Danger,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusLine {
    pub text: String,
    pub tone: Tone,
}

/// The line under the timestamp controls. A date without an offset uses
/// `analysis_zone` when a conversion has run, otherwise the zone control.
pub fn status_line(
    source: &str,
    zone: DisplayZone,
    analysis: Option<&TimestampAnalysis>,
    analysis_zone: Option<DisplayZone>,
) -> StatusLine {
    if let Some(analysis) = analysis {
        if let Some(diagnostic) = &analysis.diagnostic {
            return StatusLine {
                text: diagnostic.clone(),
                tone: Tone::Danger,
            };
        }
        let named = analysis_zone.unwrap_or(zone);
        return StatusLine {
            text: format!("Detected: {}", detected_description(analysis, named)),
            tone: Tone::Success,
        };
    }
    if source.trim().is_empty() {
        StatusLine {
            text: "Supports Unix seconds, Unix milliseconds, ISO 8601, and yyyy-MM-dd HH:mm:ss input.".to_string(),
            tone: Tone::Muted,
        }
    } else {
        StatusLine {
            text: format!(
                "Ready to convert using {} interpretation for timezone-less dates.",
                zone.label()
            ),
            tone: Tone::Info,
        }
    }
}

pub fn should_recompute(has_analysis: bool, source: &str) -> bool {
    has_analysis || !source.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use timestamp_core::TimestampEngine;

    #[test]
    fn unix_seconds_fill_local_utc_and_both_units() {
        let engine = TimestampEngine::new();
        let analysis = engine.analyze("1700000000", TimestampUnit::Automatic, TimeZone::UTC);
        assert!(analysis.diagnostic.is_none());
        let cards = result_cards(&analysis);
        assert_eq!(cards.len(), 4);
        assert_eq!(cards[1].value.contains("UTC") || cards[1].id == "utc", true);
        assert!(cards.iter().any(|card| card.value == "1700000000"));
        assert!(cards.iter().any(|card| card.value == "1700000000000"));
        assert_eq!(
            detected_description(&analysis, DisplayZone::Local),
            "Unix seconds"
        );
    }

    #[test]
    fn a_bare_date_names_the_interpretation_zone() {
        let engine = TimestampEngine::new();
        let analysis = engine.analyze(
            "2026-08-12 15:30:00",
            TimestampUnit::Automatic,
            TimeZone::UTC,
        );
        assert!(analysis.diagnostic.is_none(), "{:?}", analysis.diagnostic);
        assert_eq!(
            detected_description(&analysis, DisplayZone::Utc),
            "date/time (UTC interpretation when no offset is present)"
        );
    }

    #[test]
    fn status_follows_analysis_then_the_draft() {
        let empty = status_line("", DisplayZone::Local, None, None);
        assert_eq!(empty.tone, Tone::Muted);
        assert!(empty.text.contains("ISO 8601"));

        let ready = status_line("1700000000", DisplayZone::Utc, None, None);
        assert_eq!(ready.tone, Tone::Info);
        assert!(ready.text.contains("UTC"));

        let engine = TimestampEngine::new();
        let analysis = engine.analyze("1700000000", TimestampUnit::Automatic, TimeZone::UTC);
        let detected = status_line("1700000000", DisplayZone::Local, Some(&analysis), Some(DisplayZone::Utc));
        assert_eq!(detected.tone, Tone::Success);
        assert_eq!(detected.text, "Detected: Unix seconds");

        let spaces = status_line("  ", DisplayZone::Local, None, None);
        assert_eq!(spaces.tone, Tone::Muted);
        assert!(should_recompute(false, "1700000000"));
        assert!(!should_recompute(false, "  "));
        assert!(should_recompute(true, ""));
    }
}
