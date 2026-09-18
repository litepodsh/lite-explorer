mod containers;
mod documents;
mod mail;
#[allow(clippy::module_inception)]
mod search;
mod text;

pub(crate) use search::*;
