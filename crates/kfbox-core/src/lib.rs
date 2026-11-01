//! kfbox's engine: analyse a song, place cues by rules, write rekordbox XML.
//!
//! Decoding and analysis come from rbxport's `rbl-audio` and
//! `rbl-analysis`, and rekordbox's own library is read with `rbl-db`,
//! `rbl-index` and `rbl-anlz`. Nothing here knows about the GUI.

pub mod cache;
pub mod master;
pub mod rekordbox;
pub mod rules;
pub mod safe;
pub mod track;
pub mod xml;

pub use rekordbox::RbLibrary;
pub use rules::{Applied, Cue, Rules, DEFAULT_RULES};
pub use track::Track;
pub use xml::write_xml;
