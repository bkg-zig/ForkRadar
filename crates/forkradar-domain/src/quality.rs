//! Data model of the quality area.
//!
//! The types live in the domain crate because the API server measures the
//! values and the web client displays them. The crate that probes the tools of
//! the runtime uses these types without changing them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Description of one analysis tool of the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub id: String,
    pub name: String,
    /// Language the tool analyses.
    pub language: String,
    /// Executable that has to be on the path.
    pub executable: String,
    pub installed: bool,
    pub path: Option<String>,
    /// Version string of the tool. `None` when the tool did not answer.
    pub version: Option<String>,
}

/// Result of the tool detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInventory {
    pub tools: Vec<ToolDescriptor>,
    pub detected_at: DateTime<Utc>,
}

impl ToolInventory {
    /// Tools that are not installed.
    pub fn missing(&self) -> Vec<&ToolDescriptor> {
        self.tools.iter().filter(|tool| !tool.installed).collect()
    }
}

/// One component of a quality score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreComponent {
    pub id: String,
    pub label: String,
    /// Measured value between 0 and 100. `None` means the metric is `N/A`.
    pub value: Option<f64>,
    /// Weight of the component inside the total.
    pub weight: f64,
    /// Tool and configuration that produced the value.
    pub source: String,
}

/// Quality score of a repository revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityScore {
    /// `READY` when at least one component carries a measured value.
    pub state: String,
    /// Weighted average of the available components. `None` without any value.
    pub total: Option<f64>,
    pub components: Vec<ScoreComponent>,
    /// Description of the formula, shown in the interface.
    pub formula: String,
}

impl QualityScore {
    /// Builds a score that carries no measurement.
    pub fn not_run(reason: impl Into<String>) -> Self {
        Self {
            state: "NOT_RUN".to_string(),
            total: None,
            components: vec![ScoreComponent {
                id: "status".to_string(),
                label: reason.into(),
                value: None,
                weight: 0.0,
                source: "forkradar-analysis".to_string(),
            }],
            formula: FORMAT_DESCRIPTION.to_string(),
        }
    }
}

/// Description of the score formula.
pub const FORMAT_DESCRIPTION: &str =
    "The total is the weighted average of all components with a measured value. \
     Components without a measurement are reported as N/A and do not influence the total.";

/// Computes the weighted average of the available components.
///
/// Components without a value are reported as `N/A`. They are excluded from
/// the total, so a missing measurement never becomes a zero or a hundred.
pub fn compute_score(components: &[ScoreComponent]) -> QualityScore {
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;
    for component in components {
        let Some(value) = component.value else {
            continue;
        };
        if component.weight <= 0.0 {
            continue;
        }
        weighted_sum += value * component.weight;
        weight_sum += component.weight;
    }

    if weight_sum == 0.0 {
        return QualityScore {
            state: "NOT_RUN".to_string(),
            total: None,
            components: components.to_vec(),
            formula: FORMAT_DESCRIPTION.to_string(),
        };
    }

    QualityScore {
        state: "READY".to_string(),
        total: Some(round_two(weighted_sum / weight_sum)),
        components: components.to_vec(),
        formula: FORMAT_DESCRIPTION.to_string(),
    }
}

fn round_two(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(id: &str, value: Option<f64>, weight: f64) -> ScoreComponent {
        ScoreComponent {
            id: id.to_string(),
            label: id.to_string(),
            value,
            weight,
            source: "test".to_string(),
        }
    }

    #[test]
    fn score_without_measurement_has_no_total() {
        let score = compute_score(&[component("maintainability", None, 0.3)]);
        assert_eq!(score.state, "NOT_RUN");
        assert_eq!(score.total, None);
    }

    #[test]
    fn missing_components_do_not_lower_the_total() {
        let components = vec![
            component("maintainability", Some(80.0), 0.5),
            component("complexity", None, 0.5),
        ];
        let score = compute_score(&components);
        assert_eq!(score.state, "READY");
        assert_eq!(score.total, Some(80.0));
        assert_eq!(score.components[1].value, None);
    }

    #[test]
    fn score_is_the_weighted_average() {
        let components = vec![
            component("maintainability", Some(80.0), 0.25),
            component("complexity", Some(40.0), 0.25),
            component("testing", Some(60.0), 0.5),
        ];
        let score = compute_score(&components);
        assert_eq!(score.total, Some(60.0));
    }

    #[test]
    fn score_is_deterministic() {
        let components = vec![component("maintainability", Some(73.33), 1.0)];
        let first = compute_score(&components);
        let second = compute_score(&components);
        assert_eq!(first, second);
    }
}
