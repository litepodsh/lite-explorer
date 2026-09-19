mod containers;
mod documents;
pub(crate) mod mail;
#[allow(clippy::module_inception)]
mod search;
pub(crate) mod text;

pub(crate) use search::*;
