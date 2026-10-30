//! The state that the shell shares with every area.

use dioxus::prelude::*;
use forkradar_domain::api::{SessionUser, StatusResponse};

use crate::nav::View;

/// Kind of a message of the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Success,
    Error,
}

/// A short message above the content of an area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
}

impl Notice {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Info,
            message: message.into(),
        }
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Success,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Error,
            message: message.into(),
        }
    }

    /// The class of the message in the design system.
    pub fn css_class(&self) -> &'static str {
        match self.kind {
            NoticeKind::Error => "notice notice--error",
            NoticeKind::Info | NoticeKind::Success => "notice",
        }
    }
}

/// The state that every area of the application uses.
#[derive(Clone, Copy, PartialEq)]
pub struct AppState {
    /// The area that is shown.
    pub view: Signal<View>,
    /// The signed in user, if the browser holds a session.
    pub session: Signal<Option<SessionUser>>,
    /// The state of the server and of its integrations.
    pub status: Signal<Option<StatusResponse>>,
    /// The last message of the shell.
    pub notice: Signal<Option<Notice>>,
}

impl AppState {
    /// Creates the state of the shell.
    pub fn new() -> Self {
        Self {
            view: Signal::new(View::Overview),
            session: Signal::new(None),
            status: Signal::new(None),
            notice: Signal::new(None),
        }
    }

    /// Shows an area and writes its identifier into the address of the browser.
    pub fn navigate(&self, view: View) {
        self.view.set(view);
        self.notice.set(None);
        crate::browser::set_hash(view.slug());
    }

    /// Shows a message above the content.
    pub fn notify(&self, notice: Notice) {
        self.notice.set(Some(notice));
    }

    /// Removes the message of the shell.
    pub fn clear_notice(&self) {
        self.notice.set(None);
    }

    /// The user that is signed in, if there is one.
    pub fn user(&self) -> Option<SessionUser> {
        self.session.read().clone()
    }
}
