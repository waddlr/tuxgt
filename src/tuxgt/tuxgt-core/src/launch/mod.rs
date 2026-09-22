mod env;
mod runners;
mod spec;
mod types;

pub(crate) use env::*;
pub(crate) use runners::*;
#[cfg(test)]
pub(crate) use spec::*;
pub(crate) use types::*;

pub use env::apply_mod_env;
pub use spec::build_launch_spec;
pub use types::{game_launch_needs, is_argv_wrapper, LaunchNeeds, LaunchPaths, LaunchSpec};

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
#[cfg(test)]
mod tests_2;
#[cfg(test)]
mod tests_3;
#[cfg(test)]
mod tests_4;
