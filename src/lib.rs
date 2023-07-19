#![warn(
    clippy::pedantic,
    clippy::perf,
    clippy::style,
    clippy::suspicious,
    clippy::complexity
)]

extern crate async_trait;

pub(crate) mod common;
pub mod sync;
pub mod unsync;

pub use unsync::Mnemosyne;
pub use sync::MnemoSync;
