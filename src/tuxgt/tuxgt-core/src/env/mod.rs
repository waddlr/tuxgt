mod global;
pub mod knobs;
mod persist;
mod resolve;
mod types;

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) use types::*;

pub use global::{enabled_global_env_pairs, retire_environment_d};
pub use knobs::KNOBS;
pub use persist::{
    count_set_env, custom_env, disable_knob, enable_knob, env_keys, knob_rows, knob_values,
    migrate_env, remove_custom, set_custom, set_knob, unset_knob,
};
pub use persist::{
    disable_global_knob, enable_global_knob, global_knobs, set_global_knob, unset_global_knob,
};
pub use resolve::{
    effective_knob_value, effective_platform, knob_is_unmanaged, knob_source, live_knob_value,
};
pub use types::{
    enabled_knobs, find_enabled_knob, find_knob, proton_ge_cachy, resolve_value, scope_applies,
    scopes_display, validate_custom_key, EnvKey, EnvKnob, EnvKnobValue, EnvScope, Flavor, KnobRow,
    KnobSource, Scope,
};

#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
