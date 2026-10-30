//! Static analysis adapters for ForkRadar.
//!
//! The crate detects the analysis tools of the runtime and computes quality
//! scores from measured values. A tool that is not installed never produces a
//! measurement. The score stays `NOT_RUN` until a real tool delivered values.

use std::path::PathBuf;
use std::time::Duration;

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
    /// Detects the tools of the current runtime.
    ///
    /// The detection reads the path and asks every found tool for its version.
    /// No repository code is executed.
    pub async fn detect() -> Self {
        let known = known_tools();
        let mut tools = Vec::with_capacity(known.len());
        for (id, name, language, executable) in known {
            let path = find_on_path(&executable);
            let version = match &path {
                Some(path) => probe_version(path).await,
                None => None,
            };
            tools.push(ToolDescriptor {
                id,
                name,
                language: language.to_string(),
                executable: executable.to_string(),
                installed: path.is_some(),
                path: path.map(|path| path.to_string_lossy().to_string()),
                version,
            });
        }
        Self {
            tools,
            detected_at: Utc::now(),
        }
    }

    /// Tools that are not installed.
    pub fn missing(&self) -> Vec<&ToolDescriptor> {
        self.tools
            .iter()
            .filter(|tool| !tool.installed)
            .collect()
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
            components: Vec::new(),
            formula: FORMAT_DESCRIPTION.to_string(),
        }
        .with_reason(reason)
    }

    fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.components = vec![ScoreComponent {
            id: "status".to_string(),
            label: reason.into(),
            value: None,
            weight: 0.0,
            source: "forkradar-analysis".to_string(),
        }];
        self
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

/// The tools that ForkRadar knows.
fn known_tools() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        ("clippy", "Clippy", "Rust", "cargo-clippy"),
        ("rustfmt", "rustfmt", "Rust", "rustfmt"),
        ("cargo-audit", "cargo-audit", "Rust", "cargo-audit"),
        ("cargo-deny", "cargo-deny", "Rust", "cargo-deny"),
        ("eslint", "ESLint", "JavaScript", "eslint"),
        ("tsc", "TypeScript compiler", "TypeScript", "tsc"),
        ("ruff", "Ruff", "Python", "ruff"),
        ("radon", "Radon", "Python", "radon"),
        ("bandit", "Bandit", "Python", "bandit"),
        ("go", "go vet", "Go", "go"),
        ("staticcheck", "Staticcheck", "Go", "staticcheck"),
        ("checkstyle", "Checkstyle", "Java", "checkstyle"),
        ("pmd", "PMD", "Java", "pmd"),
        ("spotbugs", "SpotBugs", "Java", "spotbugs"),
    ]
}

/// Searches the executable on the path of the process.
fn find_on_path(executable: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(executable);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Asks the tool for its version.
async fn probe_version(path: &PathBuf) -> Option<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(path).arg("--version").output(),
    )
    .await
    .ok()?
    .ok()?;
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    let first_line = text.lines().next()?.trim().to_string();
    if first_line.is_empty() {
        None
    } else {
        Some(first_line.chars().take(160).collect())
    }
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
