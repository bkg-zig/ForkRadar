//! Conversion of the feature list into an export table.
//!
//! The table holds the rows that the interface currently shows. The frontend
//! passes the filtered and sorted list, so the file matches the screen.

use forkradar_domain::api::FeatureSuggestion;

use crate::table::{Cell, Table};

/// Builds the export table of the feature list.
pub fn feature_table(features: &[FeatureSuggestion]) -> Table {
    let mut table = Table::new(
        "Funktionen",
        vec![
            "Titel".to_string(),
            "Bereich".to_string(),
            "Beschreibung".to_string(),
            "Nutzen".to_string(),
            "Aufwand".to_string(),
            "Status".to_string(),
            "Stimmen".to_string(),
            "Ranking-Score".to_string(),
            "Effizienz".to_string(),
            "Erstellt".to_string(),
        ],
    );

    for feature in features {
        table.push(vec![
            Cell::text(feature.title.clone()),
            Cell::text(feature.area.clone()),
            Cell::text(feature.description.clone()),
            Cell::Integer(feature.impact as i64),
            Cell::Integer(feature.effort as i64),
            Cell::text(feature.status_label.clone()),
            Cell::Integer(feature.votes),
            Cell::Decimal(feature.score),
            Cell::Decimal(feature.roi),
            Cell::text(feature.created_at.to_rfc3339()),
        ]);
    }

    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use forkradar_domain::features::ScoreBreakdown;
    use uuid::Uuid;

    fn suggestion(title: &str) -> FeatureSuggestion {
        FeatureSuggestion {
            id: Uuid::from_u128(5),
            title: title.to_string(),
            description: "Beschreibung".to_string(),
            area: "Oberfläche".to_string(),
            impact: 8,
            effort: 3,
            status: "PROPOSED".to_string(),
            status_label: "Vorgeschlagen".to_string(),
            votes: 2,
            voted_by_me: true,
            roi: 2.6666667,
            score: 71.5,
            breakdown: ScoreBreakdown {
                impact: 28.0,
                effort: 20.0,
                votes: 5.0,
                status: 0.0,
            },
            created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            updated_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        }
    }

    #[test]
    fn the_table_holds_one_row_per_suggestion() {
        let features = vec![suggestion("A"), suggestion("B")];
        let table = feature_table(&features);
        assert_eq!(table.len(), 2);
        assert_eq!(table.headers.len(), 10);
    }

    #[test]
    fn the_title_column_holds_the_title() {
        let table = feature_table(&[suggestion("Schnellstart")]);
        assert_eq!(table.rows[0][0], Cell::text("Schnellstart"));
    }

    #[test]
    fn the_score_column_holds_the_server_value() {
        let table = feature_table(&[suggestion("A")]);
        assert_eq!(table.rows[0][7], Cell::Decimal(71.5));
    }

    #[test]
    fn an_empty_list_produces_an_empty_table() {
        let table = feature_table(&[]);
        assert!(table.is_empty());
        assert_eq!(table.headers.len(), 10);
    }
}
