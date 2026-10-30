//! The sections of the application.
//!
//! The frontend uses the hash of the address bar instead of a router crate.
//! The hash keeps the section after a reload and allows a direct link.

/// One section of the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Overview,
    Network,
    Explorer,
    Quality,
    Security,
    Upstream,
    Scans,
    Watchlists,
    Alerts,
    Settings,
    Features,
}

impl View {
    /// All sections in the order of the navigation.
    pub const ALL: [View; 11] = [
        View::Overview,
        View::Network,
        View::Explorer,
        View::Quality,
        View::Security,
        View::Upstream,
        View::Scans,
        View::Watchlists,
        View::Alerts,
        View::Settings,
        View::Features,
    ];

    /// The value in the address bar.
    pub fn slug(self) -> &'static str {
        match self {
            View::Overview => "overview",
            View::Network => "network",
            View::Explorer => "explorer",
            View::Quality => "quality",
            View::Security => "security",
            View::Upstream => "upstream",
            View::Scans => "scans",
            View::Watchlists => "watchlists",
            View::Alerts => "alerts",
            View::Settings => "settings",
            View::Features => "features",
        }
    }

    /// The label in the navigation.
    pub fn label(self) -> &'static str {
        match self {
            View::Overview => "Overview",
            View::Network => "Repository Network",
            View::Explorer => "Update Explorer",
            View::Quality => "Quality Center",
            View::Security => "Security Center",
            View::Upstream => "Upstream Comparison",
            View::Scans => "Scan Manager",
            View::Watchlists => "Watchlists",
            View::Alerts => "Alerts",
            View::Settings => "Profile & Settings",
            View::Features => "New Features & Ranking",
        }
    }

    /// A short text that describes the purpose of the section.
    pub fn lead(self) -> &'static str {
        match self {
            View::Overview => {
                "Kennzahlen aus den gespeicherten Scans und der Zustand der Integrationen."
            }
            View::Network => {
                "Beziehungen zwischen den gefundenen Repositorys, getrennt nach Art und Verifikation."
            }
            View::Explorer => {
                "Gefundene Repositorys eines Scans als kompakte Tabelle mit Filtern und Export."
            }
            View::Quality => "Analysewerkzeuge des Servers und die Qualitätswerte der Messungen.",
            View::Security => {
                "Sicherheitsbefunde aus verwalteten Datenquellen. Ohne Quelle bleiben die Werte leer."
            }
            View::Upstream => "Upstream-Bezüge der gefundenen Forks und ihre Verifikation.",
            View::Scans => "Scan-Läufe, ihre Teilaufgaben und die Wiederaufnahme nach Fehlern.",
            View::Watchlists => "Beobachtete Repositorys und die Scan-Konfiguration.",
            View::Alerts => "Benachrichtigungsregeln und die Ereignisse, die der Server erzeugt hat.",
            View::Settings => "Konto, Sitzung, Serverstatus und Darstellungsoptionen.",
            View::Features => {
                "Vorschläge, Ranking, Stimmen, Statuspflege und Audit der letzten 30 Tage."
            }
        }
    }

    /// Reads the section from the value in the address bar.
    pub fn from_slug(value: &str) -> View {
        let normalized = value.trim().trim_start_matches('#').to_ascii_lowercase();
        View::ALL
            .into_iter()
            .find(|view| view.slug() == normalized)
            .unwrap_or(View::Overview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_has_a_unique_slug() {
        let mut slugs: Vec<&str> = View::ALL.iter().map(|view| view.slug()).collect();
        slugs.sort_unstable();
        let count = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), count);
    }

    #[test]
    fn unknown_slug_falls_back_to_overview() {
        assert_eq!(View::from_slug("#unbekannt"), View::Overview);
        assert_eq!(View::from_slug("#features"), View::Features);
        assert_eq!(View::from_slug(""), View::Overview);
    }
}
