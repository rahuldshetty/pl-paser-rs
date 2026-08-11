//! Extraction pipeline (upstream `src/extractors/`).

pub mod content;
pub mod custom;
pub mod custom_data;
pub mod generic;
pub mod lead_image;
pub mod next_page;
#[cfg(feature = "fallback")]
pub mod readability_fallback;
pub mod root;
pub mod scoring;
pub mod transforms;
