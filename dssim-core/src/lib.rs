//! The library interface is awfully abstract, because it strives to efficiently, and very accurately,
//! support several pixel types. It also allows replacing some parts of the algorithm with different implementations
//! (if you need higher accuracy or higher speed).
#![doc(html_logo_url = "https://kornel.ski/dssim/logo.png")]

mod dssim;
mod image;
mod blur;
mod ffi;
mod val;
mod linear;
mod tolab;
pub use crate::dssim::*;
pub use crate::image::*;
pub use crate::linear::*;
