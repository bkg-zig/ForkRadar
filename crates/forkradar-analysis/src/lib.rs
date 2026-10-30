//! Static analysis adapters for ForkRadar.
//!
//! The crate detects the analysis tools of the runtime and computes quality
//! scores from measured values. A tool that is not installed never produces a
//! measurement. The score stays `NOT_RUN` until a real tool delivered values.
//!
//! The data model of the quality area lives in `forkradar-domain`, because the
//! web client renders the same values without running a tool.

use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;

// The quality model lives in `forkradar-domain`. This crate re-exports it, so
// every type that the public functions of this crate return or accept stays
// nameable through this crate.
pub use forkradar_domain::quality::{
    compute_score, QualityScore, ScoreComponent, ToolDescriptor, ToolInventory, FORMAT_DESCRIPTION,
};

/// Detects the tools of the current runtime.
///
/// The detection reads the path and asks every found tool for its version.
/// No repository code is executed.
pub async fn detect_tools() -> ToolInventory {
    let known = known_tools();
    let mut tools = Vec::with_capacity(known.len());
    for (id, name, language, executable) in known {
        let path = find_on_path(executable);
        let version = match &path {
            Some(path) => probe_version(path).await,
            None => None,
        };
        tools.push(ToolDescriptor {
            id: id.to_string(),
            name: name.to_string(),
            language: language.to_string(),
            executable: executable.to_string(),
            installed: path.is_some(),
            path: path.map(|path| path.to_string_lossy().to_string()),
            version,
        });
    }
    ToolInventory {
        tools,
        detected_at: Utc::now(),
    }
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

/// The score of a repository that no tool measured yet.
pub fn score_not_run(reason: impl Into<String>) -> QualityScore {
    QualityScore::not_run(reason)
}

/// One component of a quality score.
pub type Component = ScoreComponent;

#[cfg(test)]
mod tests {
    use super::*;
    use forkradar_domain::quality::ScoreComponent;

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

    #[test]
    fn score_not_run_has_no_total() {
        let score = score_not_run("No tool measured the revision.");
        assert_eq!(score.state, "NOT_RUN");
        assert_eq!(score.total, None);
        assert_eq!(score.components.len(), 1);
    }
}
