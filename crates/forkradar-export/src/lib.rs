//! Export formats and reading text for ForkRadar.
//!
//! The crate builds the CSV file, the XLSX workbook and the German reading
//! text. It has no dependency on the browser, so the same code runs in the
//! unit tests of the workspace and in the WebAssembly build of the frontend.

pub mod features;
pub mod speech;
pub mod table;
pub mod xlsx;
pub mod zip;

pub use features::feature_table;
pub use speech::{
    build_segments, describe, german_number, SpeechScope, SpeechSegment, SpeechSelection,
};
pub use table::{Cell, Table};
pub use xlsx::to_xlsx;
