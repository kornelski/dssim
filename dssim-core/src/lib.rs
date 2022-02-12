//! The library interface is awfully abstract, because it strives to efficiently, and very accurately,
//! support several pixel types. It also allows replacing some parts of the algorithm with different implementations
//! (if you need higher accuracy or higher speed).
#![doc(html_logo_url = "https://kornel.ski/dssim/logo.png")]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::new_without_default)]

mod blur;
mod c_api;
mod dssim;
/// cbindgen:ignore
mod ffi;
mod image;
mod linear;
mod tolab;
mod val;
#[cfg(not(feature = "threads"))]
mod lieon;

pub use crate::dssim::*;
pub use crate::image::*;
pub use crate::linear::*;
