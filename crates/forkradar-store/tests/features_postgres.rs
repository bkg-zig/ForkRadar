//! Integration test of the feature store against a real PostgreSQL.
//!
//! The test needs a database. Set `FORKRADAR_TEST_DATABASE_URL` to run it:
//!
//! ```text
//! FORKRADAR_TEST_DATABASE_URL=postgres://forkradar:forkradar_local_dev@127.0.0.1:5432/forkradar \
//!   cargo test -p forkradar-store --test features_postgres
//! ```
//!
//! Without that variable the test reports that it was skipped and passes. The
//! test creates its own users and removes them again, so it does not touch
//! the data of other tests.

use forkradar_domain::features::{ranking_score, ranking_score_total, FeatureSort, FeatureStatus};
use forkradar_store::features::{FeatureFilter, AUDIT_WINDOW_DAYS};
use forkradar_store::PgStore;

/// Connects to the test database and applies the migrations.
async fn test_store() -> Option<PgStore> {
    let url = std::env::var("FORKRADAR_TEST_DATABASE_URL").ok()?;
    let store = PgStore::connect(&url, 4)
        .await
        .expect("test database must be reachable");
    store.migrate().await.expect("migrations must apply");
    Some(store)
}

/// Builds a GitHub id that no other test uses.
fn unique_github_id(seed: u64) -> i64 {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock must be after 1970")
        .as_nanos() as i64;
    stamp ^ (seed as i64)
}

#[tokio::test]
async fn feature_store_works_against_postgres() {
    let Some(store) = test_store().await else {
        eprintln!(
            "SKIPPED: FORKRADAR_TEST_DATABASE_URL is not set, so the database test did not run"
        );
        return;
    };

    let owner = store
        .upsert_user(unique_github_id(1), "feature-owner", None, None)
        .await
        .expect("owner must be created");
    let other = store
        .upsert_user(unique_github_id(2), "feature-other", None, None)
        .await
        .expect("second user must be created");

    let id = store
        .create_feature_suggestion(
            owner.id,
            "Netzwerk-Graph exportieren",
            "Den Fork-Netzwerkgraphen als SVG exportieren.",
            "Export",
            8,
            3,
        )
        .await
        .expect("suggestion must be stored");

    // The owner sees the suggestion, the other user does not.
    let own = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter::default(),
            FeatureSort::RankingScore,
        )
        .await
        .expect("owner list must work");
    assert_eq!(own.len(), 1, "owner must see exactly one suggestion");
    assert_eq!(own[0].id, id);
    assert_eq!(own[0].status, "PROPOSED");
    assert_eq!(own[0].status_label, "Vorgeschlagen");
    assert_eq!(own[0].votes, 0);
    assert!(!own[0].voted_by_me);

    let foreign = store
        .list_feature_suggestions(
            other.id,
            &FeatureFilter::default(),
            FeatureSort::RankingScore,
        )
        .await
        .expect("list of second user must work");
    assert!(
        foreign.is_empty(),
        "a user must not see the suggestions of another user"
    );

    let foreign_detail = store
        .get_feature_suggestion(other.id, id)
        .await
        .expect("detail of second user must work");
    assert!(
        foreign_detail.is_none(),
        "a suggestion of another user must not be readable"
    );

    // The score of the server equals the score of the shared domain function.
    let expected = ranking_score(8, 3, 0, FeatureStatus::Proposed);
    assert_eq!(
        own[0].score,
        ranking_score_total(8, 3, 0, FeatureStatus::Proposed)
    );
    assert_eq!(own[0].breakdown.impact, expected.impact);
    assert_eq!(own[0].breakdown.effort, expected.effort);
    assert_eq!(own[0].breakdown.votes, expected.votes);
    assert_eq!(own[0].breakdown.status, expected.status);
    assert!((own[0].roi - 8.0 / 3.0).abs() < f64::EPSILON);

    // A second vote of the same user does not create a second row.
    assert!(
        store
            .add_feature_vote(owner.id, id)
            .await
            .expect("first vote must work"),
        "the first vote must change the data"
    );
    assert!(
        !store
            .add_feature_vote(owner.id, id)
            .await
            .expect("second vote must work"),
        "the second vote of the same user must not count twice"
    );

    let after_vote = store
        .get_feature_suggestion(owner.id, id)
        .await
        .expect("detail after vote must work")
        .expect("suggestion must still exist");
    assert_eq!(after_vote.votes, 1);
    assert!(after_vote.voted_by_me);
    assert_eq!(
        after_vote.score,
        ranking_score_total(8, 3, 1, FeatureStatus::Proposed)
    );

    // The other user cannot vote for a suggestion of another user.
    assert!(
        !store
            .add_feature_vote(other.id, id)
            .await
            .expect("foreign vote must be answered"),
        "a vote for a foreign suggestion must not be stored"
    );

    // Withdrawing works once and reports the second attempt as no change.
    assert!(
        store
            .remove_feature_vote(owner.id, id)
            .await
            .expect("withdraw must work"),
        "the first withdraw must change the data"
    );
    assert!(
        !store
            .remove_feature_vote(owner.id, id)
            .await
            .expect("second withdraw must work"),
        "the second withdraw must report no change"
    );

    // The status change belongs to the owner only.
    assert!(
        !store
            .set_feature_status(other.id, id, FeatureStatus::Shipped)
            .await
            .expect("foreign status change must work"),
        "the status of a foreign suggestion must not change"
    );
    assert!(
        store
            .set_feature_status(owner.id, id, FeatureStatus::InDevelopment)
            .await
            .expect("status change must work"),
        "the owner must change the status"
    );

    let after_status = store
        .get_feature_suggestion(owner.id, id)
        .await
        .expect("detail after status change must work")
        .expect("suggestion must still exist");
    assert_eq!(after_status.status, "IN_DEVELOPMENT");
    assert_eq!(after_status.status_label, "In Entwicklung");
    assert_eq!(
        after_status.score,
        ranking_score_total(8, 3, 0, FeatureStatus::InDevelopment)
    );

    // The filters work on the stored data.
    let by_text = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter {
                text: Some("Netzwerk".into()),
                ..FeatureFilter::default()
            },
            FeatureSort::RankingScore,
        )
        .await
        .expect("text filter must work");
    assert_eq!(by_text.len(), 1);

    let by_missing_text = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter {
                text: Some("gibt es nicht".into()),
                ..FeatureFilter::default()
            },
            FeatureSort::RankingScore,
        )
        .await
        .expect("text filter must work");
    assert!(by_missing_text.is_empty());

    let by_area = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter {
                area: Some("Export".into()),
                ..FeatureFilter::default()
            },
            FeatureSort::RankingScore,
        )
        .await
        .expect("area filter must work");
    assert_eq!(by_area.len(), 1);

    let by_wrong_area = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter {
                area: Some("Analyse".into()),
                ..FeatureFilter::default()
            },
            FeatureSort::RankingScore,
        )
        .await
        .expect("area filter must work");
    assert!(by_wrong_area.is_empty());

    let by_status = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter {
                status: Some("SHIPPED".into()),
                ..FeatureFilter::default()
            },
            FeatureSort::RankingScore,
        )
        .await
        .expect("status filter must work");
    assert!(by_status.is_empty());

    let by_impact = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter {
                min_impact: Some(9),
                ..FeatureFilter::default()
            },
            FeatureSort::RankingScore,
        )
        .await
        .expect("impact filter must work");
    assert!(by_impact.is_empty());

    let by_effort = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter {
                max_effort: Some(2),
                ..FeatureFilter::default()
            },
            FeatureSort::RankingScore,
        )
        .await
        .expect("effort filter must work");
    assert!(by_effort.is_empty());

    // The score of two different suggestions is ordered by the sort mode.
    let second_id = store
        .create_feature_suggestion(owner.id, "CSV-Export", "Filter als CSV.", "Export", 4, 9)
        .await
        .expect("second suggestion must be stored");
    let by_score = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter::default(),
            FeatureSort::RankingScore,
        )
        .await
        .expect("ranking sort must work");
    assert_eq!(by_score.len(), 2);
    assert_eq!(by_score[0].id, id, "the better score must come first");
    assert!(by_score[0].score > by_score[1].score);

    let by_roi = store
        .list_feature_suggestions(owner.id, &FeatureFilter::default(), FeatureSort::Roi)
        .await
        .expect("roi sort must work");
    // The first suggestion has impact 8 and effort 3, the second one impact 4
    // and effort 9. The relation of impact and effort is therefore higher for
    // the first suggestion.
    assert_eq!(
        by_roi[0].id, id,
        "the better relation must come first"
    );
    assert!(by_roi[0].roi > by_roi[1].roi);

    let by_impact = store
        .list_feature_suggestions(owner.id, &FeatureFilter::default(), FeatureSort::Impact)
        .await
        .expect("impact sort must work");
    assert_eq!(by_impact[0].id, id, "the higher impact must come first");

    let by_votes = store
        .list_feature_suggestions(owner.id, &FeatureFilter::default(), FeatureSort::Votes)
        .await
        .expect("votes sort must work");
    assert_eq!(by_votes.len(), 2);

    let by_newest = store
        .list_feature_suggestions(owner.id, &FeatureFilter::default(), FeatureSort::Newest)
        .await
        .expect("newest sort must work");
    assert_eq!(by_newest[0].id, second_id, "the newest one must come first");

    // The audit timeline covers thirty days and counts the events of the user.
    store
        .write_audit_event(
            Some(owner.id),
            "feature.created",
            Some(&id.to_string()),
            None,
        )
        .await
        .expect("audit event must be written");
    store
        .write_audit_event(Some(owner.id), "feature.voted", Some(&id.to_string()), None)
        .await
        .expect("audit event must be written");
    store
        .write_audit_event(
            Some(owner.id),
            "feature.status_changed",
            Some(&id.to_string()),
            None,
        )
        .await
        .expect("audit event must be written");

    let timeline = store
        .feature_audit_timeline(owner.id, AUDIT_WINDOW_DAYS)
        .await
        .expect("audit timeline must work");
    assert_eq!(timeline.len(), AUDIT_WINDOW_DAYS as usize);
    assert_eq!(timeline.last().map(|day| day.created), Some(1));
    assert_eq!(timeline.last().map(|day| day.votes), Some(1));
    assert_eq!(timeline.last().map(|day| day.status_changes), Some(1));
    assert!(
        timeline.iter().all(|day| !day.date.is_empty()),
        "every day needs a date"
    );

    let foreign_timeline = store
        .feature_audit_timeline(other.id, AUDIT_WINDOW_DAYS)
        .await
        .expect("audit timeline of second user must work");
    assert_eq!(
        foreign_timeline.iter().map(|day| day.created).sum::<i64>(),
        0,
        "the events of another user must not show up"
    );

    // Removing the users removes their suggestions and votes with it.
    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
        .bind(owner.id)
        .bind(other.id)
        .execute(store.pool())
        .await
        .expect("cleanup must work");

    let remaining = store
        .list_feature_suggestions(
            owner.id,
            &FeatureFilter::default(),
            FeatureSort::RankingScore,
        )
        .await
        .expect("list after cleanup must work");
    assert!(remaining.is_empty());
}
