mod containers;
mod documents;
pub(crate) mod mail;
#[allow(clippy::module_inception)]
mod search;
#[cfg(target_os = "macos")]
mod spotlight;
pub(crate) mod text;

pub(crate) use search::*;
