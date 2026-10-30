//! Ranking of feature suggestions.
//!
//! The score is calculated on the server so that every client shows the same
//! number. The formula is documented in `docs/features-ranking.md` and does
//! not change without a version bump of the API.

use serde::{Deserialize, Serialize};

/// Maximum score of the ranking.
pub const MAX_SCORE: f64 = 100.0;

/// Weight of the impact component.
pub const WEIGHT_IMPACT: f64 = 0.35;
/// Weight of the effort component. A low effort raises the score.
pub const WEIGHT_EFFORT: f64 = 0.25;
/// Weight of the vote component. Ten votes reach the full weight.
pub const WEIGHT_VOTES: f64 = 0.25;
/// Weight of the implementation status.
pub const WEIGHT_STATUS: f64 = 0.15;

/// Number of votes that reach the full vote weight.
pub const VOTE_SATURATION: f64 = 10.0;

/// Lifecycle state of a feature suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeatureStatus {
    Proposed,
    Planned,
    InDevelopment,
    Shipped,
}

impl FeatureStatus {
    /// Every state in the order used by the user interface.
    pub const ALL: [FeatureStatus; 4] = [
        FeatureStatus::Proposed,
        FeatureStatus::Planned,
        FeatureStatus::InDevelopment,
        FeatureStatus::Shipped,
    ];

    /// Reads a state from the stored code. Unknown codes are rejected.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "PROPOSED" | "VORGESCHLAGEN" => Some(FeatureStatus::Proposed),
            "PLANNED" | "GEPLANT" => Some(FeatureStatus::Planned),
            "IN_DEVELOPMENT" | "IN ENTWICKLUNG" => Some(FeatureStatus::InDevelopment),
            "SHIPPED" | "VERÖFFENTLICHT" | "PUBLISHED" => Some(FeatureStatus::Shipped),
            _ => None,
        }
    }

    /// The stored code of the state.
    pub fn code(self) -> &'static str {
        match self {
            FeatureStatus::Proposed => "PROPOSED",
            FeatureStatus::Planned => "PLANNED",
            FeatureStatus::InDevelopment => "IN_DEVELOPMENT",
            FeatureStatus::Shipped => "SHIPPED",
        }
    }

    /// The German label shown in the interface.
    pub fn label(self) -> &'static str {
        match self {
            FeatureStatus::Proposed => "Vorgeschlagen",
            FeatureStatus::Planned => "Geplant",
            FeatureStatus::InDevelopment => "In Entwicklung",
            FeatureStatus::Shipped => "Veröffentlicht",
        }
    }

    /// Points of the status component between zero and one.
    pub fn factor(self) -> f64 {
        match self {
            FeatureStatus::Proposed => 0.0,
            FeatureStatus::Planned => 0.4,
            FeatureStatus::InDevelopment => 0.7,
            FeatureStatus::Shipped => 1.0,
        }
    }
}

/// Sort mode of the feature list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeatureSort {
    /// By the ranking score.
    RankingScore,
    /// By the number of votes.
    Votes,
    /// By the impact.
    Impact,
    /// By the relation of impact and effort.
    Roi,
    /// By the creation date.
    Newest,
}

impl FeatureSort {
    /// Every sort mode in the order used by the user interface.
    pub const ALL: [FeatureSort; 5] = [
        FeatureSort::RankingScore,
        FeatureSort::Votes,
        FeatureSort::Impact,
        FeatureSort::Roi,
        FeatureSort::Newest,
    ];

    /// Reads the sort mode from the query parameter.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ranking_score" | "score" | "ranking" => Some(FeatureSort::RankingScore),
            "votes" | "most_votes" => Some(FeatureSort::Votes),
            "impact" | "highest_impact" => Some(FeatureSort::Impact),
            "roi" | "efficiency" | "best_roi" => Some(FeatureSort::Roi),
            "newest" | "latest" => Some(FeatureSort::Newest),
            _ => None,
        }
    }

    /// The code of the sort mode used in the query parameter.
    pub fn code(self) -> &'static str {
        match self {
            FeatureSort::RankingScore => "ranking_score",
            FeatureSort::Votes => "votes",
            FeatureSort::Impact => "impact",
            FeatureSort::Roi => "roi",
            FeatureSort::Newest => "newest",
        }
    }

    /// The German label shown in the interface.
    pub fn label(self) -> &'static str {
        match self {
            FeatureSort::RankingScore => "Ranking-Score",
            FeatureSort::Votes => "Meiste Stimmen",
            FeatureSort::Impact => "Höchster Nutzen",
            FeatureSort::Roi => "Beste Effizienz",
            FeatureSort::Newest => "Neueste zuerst",
        }
    }
}

/// The contribution of every factor to the ranking score.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    /// Points from the impact.
    pub impact: f64,
    /// Points from the effort.
    pub effort: f64,
    /// Points from the votes.
    pub votes: f64,
    /// Points from the implementation status.
    pub status: f64,
}

impl ScoreBreakdown {
    /// The sum of all points.
    pub fn total(&self) -> f64 {
        self.impact + self.effort + self.votes + self.status
    }
}

/// The relation of impact and effort. A high value means a lot of benefit for
/// little work.
pub fn roi(impact: i32, effort: i32) -> f64 {
    if effort <= 0 {
        return 0.0;
    }
    impact as f64 / effort as f64
}

/// Calculates the ranking score of a suggestion.
///
/// The score is a weighted sum of four normalized factors and therefore lies
/// between zero and [`MAX_SCORE`]. The calculation is pure, so the server and
/// every test produce the same number.
pub fn ranking_score(
    impact: i32,
    effort: i32,
    votes: i64,
    status: FeatureStatus,
) -> ScoreBreakdown {
    let impact_factor = (impact.clamp(1, 10) as f64) / 10.0;
    let effort_factor = ((11 - effort.clamp(1, 10)) as f64) / 10.0;
    let vote_factor = ((votes.max(0) as f64) / VOTE_SATURATION).min(1.0);
    let status_factor = status.factor();

    ScoreBreakdown {
        impact: round1(WEIGHT_IMPACT * impact_factor * MAX_SCORE),
        effort: round1(WEIGHT_EFFORT * effort_factor * MAX_SCORE),
        votes: round1(WEIGHT_VOTES * vote_factor * MAX_SCORE),
        status: round1(WEIGHT_STATUS * status_factor * MAX_SCORE),
    }
}

/// Calculates the ranking score of a suggestion as a single number.
pub fn ranking_score_total(impact: i32, effort: i32, votes: i64, status: FeatureStatus) -> f64 {
    let breakdown = ranking_score(impact, effort, votes, status);
    round1(breakdown.total())
}

/// Rounds to one decimal place. The rounding keeps the printed score stable.
fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_stays_between_zero_and_hundred() {
        let low = ranking_score_total(1, 10, 0, FeatureStatus::Proposed);
        let high = ranking_score_total(10, 1, 10, FeatureStatus::Shipped);
        assert!((0.0..=MAX_SCORE).contains(&low), "low score was {low}");
        assert!((0.0..=MAX_SCORE).contains(&high), "high score was {high}");
    }

    #[test]
    fn score_is_deterministic() {
        let first = ranking_score_total(7, 3, 4, FeatureStatus::Planned);
        let second = ranking_score_total(7, 3, 4, FeatureStatus::Planned);
        assert_eq!(first, second);
    }

    #[test]
    fn more_votes_raise_the_score() {
        let without = ranking_score_total(5, 5, 0, FeatureStatus::Proposed);
        let with = ranking_score_total(5, 5, 3, FeatureStatus::Proposed);
        assert!(with > without);
    }

    #[test]
    fn lower_effort_raises_the_score() {
        let heavy = ranking_score_total(5, 10, 0, FeatureStatus::Proposed);
        let light = ranking_score_total(5, 2, 0, FeatureStatus::Proposed);
        assert!(light > heavy);
    }

    #[test]
    fn status_is_part_of_the_score() {
        let proposed = ranking_score_total(5, 5, 0, FeatureStatus::Proposed);
        let shipped = ranking_score_total(5, 5, 0, FeatureStatus::Shipped);
        assert!(shipped > proposed);
    }

    #[test]
    fn breakdown_sums_to_the_total() {
        let breakdown = ranking_score(8, 4, 6, FeatureStatus::InDevelopment);
        assert_eq!(
            breakdown.total(),
            ranking_score_total(8, 4, 6, FeatureStatus::InDevelopment)
        );
    }

    #[test]
    fn status_codes_round_trip() {
        for status in FeatureStatus::ALL {
            assert_eq!(FeatureStatus::parse(status.code()), Some(status));
        }
        assert_eq!(FeatureStatus::parse("unbekannt"), None);
    }

    #[test]
    fn sort_codes_round_trip() {
        for sort in FeatureSort::ALL {
            assert_eq!(FeatureSort::parse(sort.code()), Some(sort));
        }
        assert_eq!(FeatureSort::parse("nonsense"), None);
    }

    #[test]
    fn roi_reports_the_relation_of_impact_and_effort() {
        assert_eq!(roi(8, 4), 2.0);
        assert_eq!(roi(5, 0), 0.0);
    }
}
