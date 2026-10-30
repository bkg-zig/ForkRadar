//! German reading text for the browser speech synthesis.
//!
//! The browser reads the passages one after another. Every passage belongs to
//! one suggestion, so the interface can highlight the suggestion that is
//! currently spoken.

use forkradar_domain::api::FeatureSuggestion;
use uuid::Uuid;

/// How much of a suggestion is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechScope {
    /// Title, area, values, status, votes and the description.
    Full,
    /// Title, status and the score.
    Compact,
}

/// Which part of the ranking is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechSelection {
    /// The whole list in the order it is shown.
    Ranking,
    /// The three highest ranked suggestions.
    TopThree,
    /// A single suggestion.
    Single(Uuid),
}

/// One part of the reading text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechSegment {
    /// Identifier of the suggestion the passage belongs to.
    pub feature_id: Uuid,
    /// Short label of the passage, for example `Platz 1`.
    pub label: String,
    /// The spoken text.
    pub text: String,
}

/// Builds the passages for the given selection.
///
/// The list is expected to arrive in the order the server returned it, because
/// the position in the list is part of the spoken text.
pub fn build_segments(
    features: &[FeatureSuggestion],
    selection: SpeechSelection,
    scope: SpeechScope,
) -> Vec<SpeechSegment> {
    let selected: Vec<(usize, &FeatureSuggestion)> = match selection {
        SpeechSelection::Ranking => features.iter().enumerate().collect(),
        SpeechSelection::TopThree => features.iter().enumerate().take(3).collect(),
        SpeechSelection::Single(id) => features
            .iter()
            .enumerate()
            .filter(|(_, feature)| feature.id == id)
            .collect(),
    };

    selected
        .into_iter()
        .map(|(position, feature)| match selection {
            SpeechSelection::Single(_) => SpeechSegment {
                feature_id: feature.id,
                label: feature.title.clone(),
                text: describe(feature, scope),
            },
            _ => SpeechSegment {
                feature_id: feature.id,
                label: format!("Platz {}", position + 1),
                text: describe_with_position(position + 1, feature, scope),
            },
        })
        .collect()
}

/// Builds the reading text of the whole list.
pub fn ranking_speech(features: &[FeatureSuggestion], scope: SpeechScope) -> Vec<SpeechSegment> {
    build_segments(features, SpeechSelection::Ranking, scope)
}

/// Builds the reading text of the three highest ranked suggestions.
pub fn top_three_speech(features: &[FeatureSuggestion]) -> Vec<SpeechSegment> {
    build_segments(features, SpeechSelection::TopThree, SpeechScope::Full)
}

/// Builds the reading text of a single suggestion.
pub fn feature_speech(feature: &FeatureSuggestion) -> Vec<SpeechSegment> {
    build_segments(
        std::slice::from_ref(feature),
        SpeechSelection::Single(feature.id),
        SpeechScope::Full,
    )
}

/// Builds the spoken text of one suggestion.
pub fn describe(feature: &FeatureSuggestion, scope: SpeechScope) -> String {
    let mut sentences = Vec::new();
    sentences.push(sentence(&feature.title));

    if scope == SpeechScope::Full {
        if !feature.area.trim().is_empty() {
            sentences.push(format!("Bereich {}.", feature.area));
        }
        sentences.push(format!(
            "Nutzen {} von 10. Aufwand {} von 10.",
            feature.impact, feature.effort
        ));
    }

    sentences.push(format!("Status {}.", feature.status_label));

    if scope == SpeechScope::Full {
        sentences.push(format!(
            "{} Stimme{}.",
            feature.votes,
            if feature.votes == 1 { "" } else { "n" }
        ));
        sentences.push(format!(
            "Rankingpunkte {}. Effizienz {}.",
            german_number(feature.score),
            german_number(feature.roi)
        ));
        if !feature.description.trim().is_empty() {
            sentences.push(sentence(&feature.description));
        }
    } else {
        sentences.push(format!("Rankingpunkte {}.", german_number(feature.score)));
    }

    sentences.join(" ")
}

/// Turns a text into a spoken sentence by adding a full stop when the text
/// does not end with one.
fn sentence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with(['.', '!', '?']) {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

/// Builds the spoken text of a suggestion with its position in the list.
fn describe_with_position(
    position: usize,
    feature: &FeatureSuggestion,
    scope: SpeechScope,
) -> String {
    format!("Platz {position}. {}", describe(feature, scope))
}

/// Writes a number with a comma as the decimal separator, as it is read in
/// German.
pub fn german_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let rounded = (value * 10.0).round() / 10.0;
    let text = format!("{rounded:.1}");
    let (integer, fraction) = text.split_once('.').unwrap_or((text.as_str(), "0"));
    let integer = if integer == "-0" { "0" } else { integer };
    format!("{integer},{fraction}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use forkradar_domain::features::ScoreBreakdown;

    fn feature(id: Uuid, title: &str, score: f64) -> FeatureSuggestion {
        FeatureSuggestion {
            id,
            title: title.to_string(),
            description: "Beschreibung des Vorschlags".to_string(),
            area: "Oberflaeche".to_string(),
            impact: 8,
            effort: 3,
            status: "PROPOSED".to_string(),
            status_label: "Vorgeschlagen".to_string(),
            votes: 12,
            voted_by_me: false,
            roi: 2.66,
            score,
            breakdown: ScoreBreakdown {
                impact: 28.0,
                effort: 17.5,
                votes: 25.0,
                status: 0.0,
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn sample() -> Vec<FeatureSuggestion> {
        vec![
            feature(Uuid::from_u128(1), "Erster", 90.0),
            feature(Uuid::from_u128(2), "Zweiter", 80.0),
            feature(Uuid::from_u128(3), "Dritter", 70.0),
            feature(Uuid::from_u128(4), "Vierter", 60.0),
        ]
    }

    #[test]
    fn the_ranking_produces_one_passage_per_feature() {
        let segments = ranking_speech(&sample(), SpeechScope::Full);
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0].label, "Platz 1");
        assert!(segments[0].text.starts_with("Platz 1. Erster."));
    }

    #[test]
    fn the_top_three_stops_after_three_entries() {
        let segments = top_three_speech(&sample());
        assert_eq!(segments.len(), 3);
        assert!(segments[2].text.contains("Dritter"));
    }

    #[test]
    fn a_single_feature_is_read_without_a_position() {
        let features = sample();
        let segments = feature_speech(&features[1]);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].label, "Zweiter");
        assert!(segments[0].text.starts_with("Zweiter."));
        assert!(!segments[0].text.contains("Platz"));
    }

    #[test]
    fn an_unknown_feature_produces_no_passage() {
        let segments = build_segments(
            &sample(),
            SpeechSelection::Single(Uuid::from_u128(99)),
            SpeechScope::Full,
        );
        assert!(segments.is_empty());
    }

    #[test]
    fn the_compact_scope_leaves_out_the_details() {
        let features = sample();
        let full = build_segments(
            &features,
            SpeechSelection::Single(features[0].id),
            SpeechScope::Full,
        );
        let compact = build_segments(
            &features,
            SpeechSelection::Single(features[0].id),
            SpeechScope::Compact,
        );
        assert!(full[0].text.contains("Beschreibung des Vorschlags"));
        assert!(full[0].text.contains("12 Stimmen"));
        assert!(!compact[0].text.contains("Beschreibung des Vorschlags"));
        assert!(!compact[0].text.contains("Stimmen"));
        assert!(compact[0].text.contains("Rankingpunkte"));
    }

    #[test]
    fn one_vote_is_read_without_the_plural_s() {
        let mut feature = feature(Uuid::from_u128(5), "Filter", 50.0);
        feature.votes = 1;
        let segments = feature_speech(&feature);
        assert!(segments[0].text.contains("1 Stimme."));
        assert!(!segments[0].text.contains("1 Stimmen"));
    }

    #[test]
    fn every_segment_carries_the_id_of_its_suggestion() {
        let features = sample();
        let segments = ranking_speech(&features, SpeechScope::Compact);
        for (segment, feature) in segments.iter().zip(features.iter()) {
            assert_eq!(segment.feature_id, feature.id);
        }
    }

    #[test]
    fn numbers_use_the_german_decimal_separator() {
        assert_eq!(german_number(87.55), "87,6");
        assert_eq!(german_number(90.0), "90,0");
        assert_eq!(german_number(0.0), "0,0");
    }
}
