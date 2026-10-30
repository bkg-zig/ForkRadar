//! German reading of the ranking with the speech synthesis of the browser.
//!
//! The module owns the queue of passages. Every passage belongs to one
//! suggestion, so the interface can highlight the suggestion that is read at
//! the moment. The browser API is the only source of the spoken text, the
//! server never receives it.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use dioxus::prelude::*;
use forkradar_export::speech::{SpeechScope, SpeechSegment};
use uuid::Uuid;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{SpeechSynthesis, SpeechSynthesisUtterance, SpeechSynthesisVoice};

/// Smallest speaking rate that the interface offers.
pub const MIN_RATE: f32 = 0.9;
/// Largest speaking rate that the interface offers.
pub const MAX_RATE: f32 = 1.5;
/// Rate that the interface starts with.
pub const DEFAULT_RATE: f32 = 1.0;

/// State of the reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechStatus {
    /// Nothing is read.
    Idle,
    /// The browser reads a passage.
    Speaking,
    /// The reading is paused.
    Paused,
    /// The last passage was read.
    Finished,
}

impl SpeechStatus {
    /// The German label of the state.
    pub fn label(self) -> &'static str {
        match self {
            SpeechStatus::Idle => "Bereit",
            SpeechStatus::Speaking => "Liest vor",
            SpeechStatus::Paused => "Pausiert",
            SpeechStatus::Finished => "Beendet",
        }
    }
}

/// The values of the reading that the interface shows.
#[derive(Clone, Copy)]
pub struct SpeechUi {
    /// State of the reading.
    pub status: Signal<SpeechStatus>,
    /// Identifier of the suggestion that is read at the moment.
    pub current: Signal<Option<Uuid>>,
    /// Position of the passage in the queue.
    pub index: Signal<usize>,
    /// Speaking rate.
    pub rate: Signal<f32>,
    /// Amount of detail of the passages.
    pub scope: Signal<SpeechScope>,
    /// The passages of the current selection.
    pub segments: Signal<Vec<SpeechSegment>>,
}

/// The state that the controller and the browser callbacks share.
struct Shared {
    ui: SpeechUi,
    synthesis: Option<SpeechSynthesis>,
    /// Position of the passage that is read at the moment.
    index: Cell<usize>,
    /// Callback that runs when a passage ends.
    on_end: RefCell<Option<Closure<dyn FnMut()>>>,
}

impl Shared {
    /// Writes the state of the reading.
    fn set_status(&self, status: SpeechStatus) {
        let mut signal = self.ui.status;
        signal.set(status);
    }

    /// Writes the suggestion that is read at the moment.
    fn set_current(&self, current: Option<Uuid>) {
        let mut signal = self.ui.current;
        signal.set(current);
    }

    /// Writes the position of the passage.
    fn set_index(&self, index: usize) {
        let mut signal = self.ui.index;
        signal.set(index);
        self.index.set(index);
    }
}

/// Drives the speech synthesis of the browser.
#[derive(Clone)]
pub struct SpeechController {
    shared: Rc<Shared>,
}

impl SpeechController {
    /// Builds a controller for the given interface values.
    pub fn new(ui: SpeechUi) -> Self {
        let synthesis = crate::browser::window().and_then(|window| window.speech_synthesis().ok());

        let shared = Rc::new(Shared {
            ui,
            synthesis,
            index: Cell::new(0),
            on_end: RefCell::new(None),
        });

        let weak: Weak<Shared> = Rc::downgrade(&shared);
        let callback = Closure::wrap(Box::new(move || {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            let next = {
                let count = shared.ui.segments.read().len();
                if shared.index.get() + 1 < count {
                    Some(shared.index.get() + 1)
                } else {
                    None
                }
            };
            match next {
                Some(index) => speak_at(&shared, index),
                None => {
                    shared.set_status(SpeechStatus::Finished);
                    shared.set_current(None);
                }
            }
        }) as Box<dyn FnMut()>);
        *shared.on_end.borrow_mut() = Some(callback);

        Self { shared }
    }

    /// Reports whether the browser offers a speech synthesis.
    pub fn is_available(&self) -> bool {
        self.shared.synthesis.is_some()
    }

    /// Starts the reading with the given passages.
    pub fn start(&self, segments: Vec<SpeechSegment>) {
        if segments.is_empty() {
            self.stop();
            return;
        }
        let mut signal = self.shared.ui.segments;
        signal.set(segments);
        speak_at(&self.shared, 0);
    }

    /// Reads the current passage again from the beginning.
    pub fn restart(&self) {
        let count = self.shared.ui.segments.read().len();
        if count == 0 {
            return;
        }
        let index = self.shared.index.get().min(count - 1);
        speak_at(&self.shared, index);
    }

    /// Pauses the reading.
    pub fn pause(&self) {
        if let Some(synthesis) = &self.shared.synthesis {
            synthesis.pause();
        }
        self.shared.set_status(SpeechStatus::Paused);
    }

    /// Continues the reading.
    pub fn resume(&self) {
        if let Some(synthesis) = &self.shared.synthesis {
            synthesis.resume();
        }
        self.shared.set_status(SpeechStatus::Speaking);
    }

    /// Stops the reading and clears the queue.
    pub fn stop(&self) {
        if let Some(synthesis) = &self.shared.synthesis {
            synthesis.cancel();
        }
        let mut segments = self.shared.ui.segments;
        segments.set(Vec::new());
        self.shared.set_current(None);
        self.shared.set_index(0);
        self.shared.set_status(SpeechStatus::Idle);
    }

    /// Reads the next passage.
    pub fn next(&self) {
        let count = self.shared.ui.segments.read().len();
        if count == 0 {
            return;
        }
        let index = (self.shared.index.get() + 1).min(count - 1);
        speak_at(&self.shared, index);
    }

    /// Reads the previous passage.
    pub fn previous(&self) {
        let count = self.shared.ui.segments.read().len();
        if count == 0 {
            return;
        }
        let index = self.shared.index.get().saturating_sub(1);
        speak_at(&self.shared, index);
    }

    /// Sets the speaking rate and reads the current passage again.
    pub fn set_rate(&self, rate: f32) {
        let rate = rate.clamp(MIN_RATE, MAX_RATE);
        let mut signal = self.shared.ui.rate;
        signal.set(rate);
        let status = self.shared.ui.status.read();
        if status != SpeechStatus::Idle && status != SpeechStatus::Finished {
            self.restart();
        }
    }

    /// Sets the amount of detail of the passages.
    pub fn set_scope(&self, scope: SpeechScope) {
        let mut signal = self.shared.ui.scope;
        signal.set(scope);
    }
}

/// Reads the passage at the given position.
fn speak_at(shared: &Rc<Shared>, index: usize) {
    let Some(synthesis) = shared.synthesis.clone() else {
        shared.set_status(SpeechStatus::Idle);
        return;
    };

    let (text, feature_id, rate) = {
        let segments = shared.ui.segments.read();
        let Some(segment) = segments.get(index) else {
            shared.set_status(SpeechStatus::Finished);
            return;
        };
        (
            segment.text.clone(),
            Some(segment.feature_id),
            shared.ui.rate.read(),
        )
    };

    let Ok(utterance) = SpeechSynthesisUtterance::new_with_text(&text) else {
        shared.set_status(SpeechStatus::Idle);
        return;
    };
    utterance.set_lang("de-DE");
    utterance.set_rate(rate);
    utterance.set_pitch(1.0);
    utterance.set_volume(1.0);
    if let Some(voice) = german_voice(&synthesis) {
        utterance.set_voice(Some(&voice));
    }

    let callback = shared.on_end.borrow();
    if let Some(callback) = callback.as_ref() {
        utterance.set_onend(Some(callback.as_ref().unchecked_ref()));
    }

    synthesis.cancel();
    shared.set_index(index);
    shared.set_current(feature_id);
    shared.set_status(SpeechStatus::Speaking);
    synthesis.speak(&utterance);
}

/// Looks for a voice that reads German.
fn german_voice(synthesis: &SpeechSynthesis) -> Option<SpeechSynthesisVoice> {
    let voices = synthesis.get_voices();
    let mut fallback = None;
    for voice in voices.iter() {
        let Ok(voice) = voice.dyn_into::<SpeechSynthesisVoice>() else {
            continue;
        };
        let lang = voice.lang().to_ascii_lowercase();
        if lang == "de-de" {
            return Some(voice);
        }
        if lang.starts_with("de") && fallback.is_none() {
            fallback = Some(voice);
        }
    }
    fallback
}

/// The reading text of all passages, joined for the preview.
pub fn preview_text(segments: &[SpeechSegment]) -> String {
    segments
        .iter()
        .map(|segment| segment.text.clone())
        .collect::<Vec<String>>()
        .join(" ")
}
