//! Shared model, import/export and font logic for the LEDFX editor.
//!
//! The web editor is a React/TypeScript front-end that talks to the `serve`
//! binary over JSON. Binary formats (USB.FXD and GIF) are produced and parsed
//! by this crate on the server.

pub mod export;
pub mod model;
pub mod palette;
pub mod pcf;
