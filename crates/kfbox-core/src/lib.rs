//! kfbox's engine: analyse a song, place cues by rules, write rekordbox XML.
//!
//! Decoding and analysis come from rbxport's `rbl-audio` and
//! `rbl-analysis`, and rekordbox's own library is read with `rbl-db`,
//! `rbl-index` and `rbl-anlz`. Nothing here knows about the GUI.

pub mod autocue;
pub mod cache;
pub mod import;
pub mod learning;
pub mod library;
pub mod master;
pub mod rekordbox;
pub mod rules;
pub mod safe;
pub mod scoring;
pub mod session;
#[doc(hidden)]
pub mod testing;
pub mod tools;
pub mod track;
pub mod usb;
pub mod xml;

pub use rekordbox::RbLibrary;
pub use rules::{Applied, Cue, Flag, FlagKind, Rules, DEFAULT_RULES};
pub use track::Track;
pub use xml::write_xml;

/// Bytes as lowercase hex, as a hash is shown.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}
