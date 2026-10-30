//! Small building blocks that the areas of the application share.

use dioxus::prelude::*;
use forkradar_domain::model::{JobStatus, RelationshipKind, RelationshipVerification};

/// The heading of an area with an optional row of buttons.
pub fn page_head(title: &str, lead: &str, actions: Option<Element>) -> Element {
    let actions = match actions {
        Some(actions) => rsx! {
            div { class: "page__actions", {actions} }
        },
        None => rsx! {},
    };
    rsx! {
        header { class: "page__head",
            div {
                h1 { class: "page__title", "{title}" }
                p { class: "page__lead", "{lead}" }
            }
            {actions}
        }
    }
}

/// A panel with a heading and a body.
pub fn panel(title: &str, hint: Option<&str>, body: Element) -> Element {
    let hint = match hint {
        Some(hint) => rsx! {
            p { class: "panel__hint", "{hint}" }
        },
        None => rsx! {},
    };
    rsx! {
        section { class: "panel",
            header { class: "panel__head",
                h2 { class: "panel__title", "{title}" }
                {hint}
            }
            div { class: "panel__body", {body} }
        }
    }
}

/// A panel whose body has no padding. Tables use it.
pub fn panel_flush(title: &str, hint: Option<&str>, body: Element) -> Element {
    let hint = match hint {
        Some(hint) => rsx! {
            p { class: "panel__hint", "{hint}" }
        },
        None => rsx! {},
    };
    rsx! {
        section { class: "panel",
            header { class: "panel__head",
                h2 { class: "panel__title", "{title}" }
                {hint}
            }
            div { class: "panel__body panel__body--flush", {body} }
        }
    }
}

/// The kind of a state block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateKind {
    /// Something went wrong.
    Error,
    /// Something is missing.
    Warning,
    /// There is simply nothing to show.
    Muted,
}

impl StateKind {
    /// The class of the block.
    fn css_class(self) -> &'static str {
        match self {
            StateKind::Error => "state state--error",
            StateKind::Warning => "state state--warning",
            StateKind::Muted => "state state--muted",
        }
    }
}

/// A block that explains the state of a section.
pub fn state_block(kind: StateKind, title: &str, body: Element) -> Element {
    rsx! {
        div { class: "{kind.css_class()}",
            p { class: "state__title", "{title}" }
            div { class: "state__body", {body} }
        }
    }
}

/// A block for a failed request.
pub fn error_state(message: &str) -> Element {
    state_block(
        StateKind::Error,
        "Die Anfrage ist fehlgeschlagen",
        rsx! { p { "{message}" } },
    )
}

/// A block for a section that needs a configuration.
pub fn not_configured(message: &str) -> Element {
    state_block(
        StateKind::Warning,
        "Nicht konfiguriert",
        rsx! { p { "{message}" } },
    )
}

/// A block for a section that was not carried out.
pub fn not_run(message: &str) -> Element {
    state_block(
        StateKind::Muted,
        "Nicht ausgeführt",
        rsx! { p { "{message}" } },
    )
}

/// A block for an empty list.
pub fn empty_state(title: &str, message: &str) -> Element {
    state_block(StateKind::Muted, title, rsx! { p { "{message}" } })
}

/// The bar that shows that a request runs.
pub fn loading(label: &str) -> Element {
    rsx! {
        div { class: "loading",
            div { class: "loading__bar" }
            p { class: "inline-note", "{label}" }
        }
    }
}

/// A small label with an optional tone.
pub fn badge(text: &str, tone: Option<&str>) -> Element {
    let class = match tone {
        Some(tone) => format!("badge badge--{tone}"),
        None => "badge".to_string(),
    };
    rsx! {
        span { class: "{class}", "{text}" }
    }
}

/// The label of a state of an integration.
pub fn integration_badge(state: &str) -> Element {
    let tone = match state {
        "READY" => "ready",
        "NOT_CONFIGURED" => "not-configured",
        "NOT_RUN" => "not-run",
        "BLOCKED" => "blocked",
        _ => "unverified",
    };
    badge(state, Some(tone))
}

/// The label of the state of a job.
pub fn job_badge(status: JobStatus) -> Element {
    let tone = match status {
        JobStatus::Discovering => "discovering",
        JobStatus::Queued => "queued",
        JobStatus::Running => "running",
        JobStatus::Partial => "partial",
        JobStatus::Completed => "completed",
        JobStatus::Failed => "failed",
        JobStatus::Cancelled => "cancelled",
    };
    badge(status.as_str(), Some(tone))
}

/// The German name of a kind of relationship.
pub fn relationship_label(kind: RelationshipKind) -> &'static str {
    match kind {
        RelationshipKind::MainSource => "Beobachtetes Repository",
        RelationshipKind::GithubReportedParent => "Von GitHub gemeldeter Elternteil",
        RelationshipKind::GithubReportedSource => "Von GitHub gemeldete Quelle",
        RelationshipKind::VerifiedImmediateParent => "Geprüfter unmittelbarer Elternteil",
        RelationshipKind::ForkNetworkMember => "Mitglied des Fork-Netzwerks",
        RelationshipKind::SharedCommitAncestry => "Gemeinsame Commit-Historie",
        RelationshipKind::SimilarContent => "Ähnlicher Inhalt",
        RelationshipKind::Unclear => "Unklar",
    }
}

/// The German name of the verification of a relationship.
pub fn verification_label(verification: RelationshipVerification) -> &'static str {
    match verification {
        RelationshipVerification::ConfirmedByForkListing => "Aus der Fork-Liste bestätigt",
        RelationshipVerification::ReportedByGithub => "Von GitHub gemeldet",
        RelationshipVerification::DerivedFromSourceChain => "Aus der Quellenkette abgeleitet",
        RelationshipVerification::Unverified => "Nicht geprüft",
    }
}

/// One number with its label.
pub fn metric(label: &str, value: &str, note: Option<&str>) -> Element {
    let note = match note {
        Some(note) => rsx! {
            p { class: "metric__note", "{note}" }
        },
        None => rsx! {},
    };
    rsx! {
        div { class: "metric",
            p { class: "metric__label", "{label}" }
            p { class: "metric__value", "{value}" }
            {note}
        }
    }
}

/// A list of name and value pairs.
pub fn definition_list(rows: Vec<(String, String)>) -> Element {
    let items: Vec<Element> = rows
        .into_iter()
        .map(|(name, value)| {
            rsx! {
                div { class: "definition-list__row",
                    dt { "{name}" }
                    dd { "{value}" }
                }
            }
        })
        .collect();
    rsx! {
        dl { class: "definition-list", {items} }
    }
}
