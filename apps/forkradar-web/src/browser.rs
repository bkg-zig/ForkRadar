//! Helpers for the browser APIs that the shell needs.
//!
//! The module keeps every call to `web_sys` in one place. The rest of the
//! frontend works with plain values, so a missing browser API becomes a
//! readable error message instead of a panic.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Window};

/// Returns the window of the browser, if the code runs in one.
pub fn window() -> Option<Window> {
    web_sys::window()
}

/// Reads the fragment of the address, without the leading hash.
pub fn current_hash() -> String {
    let Some(window) = window() else {
        return String::new();
    };
    let Ok(location) = window.location() else {
        return String::new();
    };
    location
        .hash()
        .unwrap_or_default()
        .trim_start_matches('#')
        .to_string()
}

/// Writes the fragment of the address.
///
/// The browser adds the `hashchange` event, so the shell follows the change
/// without a second code path.
pub fn set_hash(slug: &str) {
    let Some(window) = window() else {
        return;
    };
    let Ok(location) = window.location() else {
        return;
    };
    let _ = location.set_hash(slug);
}

/// Registers a listener for the `hashchange` event.
///
/// The returned closure keeps the listener alive. The caller stores it in a
/// hook, so it is dropped together with the component.
pub fn listen_hash_change(
    callback: impl Fn(String) + 'static,
) -> Option<Closure<dyn FnMut(web_sys::Event)>> {
    let window = window()?;
    let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        callback(current_hash());
    }) as Box<dyn FnMut(web_sys::Event)>);
    window
        .add_event_listener_with_callback("hashchange", closure.as_ref().unchecked_ref())
        .ok()?;
    Some(closure)
}

/// Downloads bytes as a file.
///
/// The function builds a temporary link with the `download` attribute, clicks
/// it and removes it again. The object URL is released right afterwards.
pub fn download_bytes(file_name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
    let window = window().ok_or("Der Browser stellt kein Fenster bereit.")?;
    let document = window
        .document()
        .ok_or("Der Browser stellt kein Dokument bereit.")?;

    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes));

    let mut options = BlobPropertyBag::new();
    options.type_(mime);
    let blob = Blob::new_with_u8_slice_sequence_and_options(parts.as_ref(), &options)
        .map_err(|error| format!("Der Dateiinhalt konnte nicht erzeugt werden: {error:?}"))?;

    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|error| format!("Der Download-Link konnte nicht erzeugt werden: {error:?}"))?;

    let result = (|| -> Result<(), String> {
        let anchor = document
            .create_element("a")
            .map_err(|error| format!("Der Download-Link konnte nicht erzeugt werden: {error:?}"))?;
        let anchor: HtmlAnchorElement = anchor
            .dyn_into()
            .map_err(|_| "Der Download-Link konnte nicht erzeugt werden.".to_string())?;
        anchor.set_href(&url);
        anchor.set_download(file_name);
        anchor.style().set_property("display", "none");
        let body = document
            .body()
            .ok_or("Der Browser stellt keinen Seitenkörper bereit.")?;
        body.append_child(&anchor)
            .map_err(|error| format!("Der Download konnte nicht gestartet werden: {error:?}"))?;
        anchor.click();
        body.remove_child(&anchor)
            .map_err(|error| format!("Der Download konnte nicht beendet werden: {error:?}"))?;
        Ok(())
    })();

    let _ = web_sys::Url::revoke_object_url(&url);
    result
}

/// Sends the browser to another page of the same application.
pub fn navigate_to(path: &str) {
    let Some(window) = window() else {
        return;
    };
    let Ok(location) = window.location() else {
        return;
    };
    let _ = location.set_href(path);
}
