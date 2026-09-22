use super::*;
use crate::{Error, PluginHost, Result};

/// Platform classes a knob applies to. Empty knob scopes mean any platform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    Proton,
    Wine,
    Native,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::Proton => "proton",
            Scope::Wine => "wine",
            Scope::Native => "native",
        }
    }
}

/// Proton flavors a knob is tagged for. Empty flavors = any Proton.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flavor {
    GeCachy,
}

impl Flavor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Flavor::GeCachy => "ge_cachy",
        }
    }
}

/// Same match as launch `proton_optiscaler_flavor`.
pub fn proton_ge_cachy(proton: Option<&str>) -> bool {
    proton.is_some_and(|p| {
        let l = p.to_ascii_lowercase();
        l.contains("cachy") || l.contains("ge-proton")
    })
}

/// Stored knob row (game or global). Disabled keeps `value`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnobRow {
    pub knob: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnobSource {
    None,
    Unmanaged,
    Global,
    Game,
}

impl KnobSource {
    pub fn as_str(self) -> &'static str {
        match self {
            KnobSource::None => "",
            KnobSource::Unmanaged => "unmanaged",
            KnobSource::Global => "global",
            KnobSource::Game => "game",
        }
    }
}

/// Storage scope of one env key: game knob, game custom pair, or global knob.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvScope {
    Game,
    Custom,
    Global,
}

impl EnvScope {
    pub fn as_str(self) -> &'static str {
        match self {
            EnvScope::Game => "game",
            EnvScope::Custom => "custom",
            EnvScope::Global => "global",
        }
    }
}

/// One resolved env key: stored value, enable flag, and where it came from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvKey {
    pub value: String,
    pub enabled: bool,
    pub scope: EnvScope,
}

/// One allowed knob value: description plus the env vars it sets.
pub struct EnvKnobValue {
    pub value: &'static str,
    pub help: &'static str,
    pub env: &'static [(&'static str, &'static str)],
}

/// A registered env knob. Exactly one of `freeform` or non-empty `values`;
/// an unset knob writes no env.
pub struct EnvKnob {
    pub id: &'static str,
    pub help: &'static str,
    pub scopes: &'static [Scope],
    pub flavors: &'static [Flavor],
    pub freeform: Option<&'static str>,
    pub values: &'static [EnvKnobValue],
}

impl EnvKnob {
    /// The env pairs a stored value produces. `value` must already be valid.
    pub fn env_pairs(&self, value: &str) -> Option<Vec<(String, String)>> {
        if let Some(var) = self.freeform {
            return Some(vec![(var.to_string(), value.to_string())]);
        }
        let found = self.values.iter().find(|v| v.value == value)?;
        Some(
            found
                .env
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        )
    }

    pub fn value_help(&self, value: &str) -> Option<&'static str> {
        if self.freeform.is_some() {
            return Some(self.help);
        }
        self.values
            .iter()
            .find(|v| v.value == value)
            .map(|v| v.help)
    }

    /// GUI/CLI label: env var names, not `id`. Multi-var joined with ` · `.
    pub fn env_label(&self) -> String {
        self.env_vars().join(" · ")
    }

    pub fn env_vars(&self) -> Vec<&'static str> {
        if let Some(var) = self.freeform {
            return vec![var];
        }
        let mut vars = Vec::new();
        for v in self.values {
            for (var, _) in v.env {
                if !vars.contains(var) {
                    vars.push(*var);
                }
            }
        }
        vars
    }

    pub fn ge_cachy_only(&self) -> bool {
        self.flavors.contains(&Flavor::GeCachy)
    }
}

pub fn find_knob(id: &str) -> Option<&'static EnvKnob> {
    knobs::KNOBS.iter().find(|k| k.id == id)
}

/// Knobs when the `env` plugin is enabled, in table order.
pub fn enabled_knobs(host: &PluginHost) -> Vec<&'static EnvKnob> {
    if host.is_enabled("env") {
        knobs::KNOBS.iter().collect()
    } else {
        Vec::new()
    }
}

/// Resolve one knob when the `env` plugin is enabled; a disabled plugin's
/// knobs are not offered.
pub fn find_enabled_knob(host: &PluginHost, id: &str) -> Option<&'static EnvKnob> {
    enabled_knobs(host).into_iter().find(|k| k.id == id)
}

pub fn scope_applies(scopes: &[Scope], platform: &str) -> bool {
    scopes.is_empty() || scopes.iter().any(|s| s.as_str() == platform)
}

/// Scopes as a display string: `all` when unrestricted.
pub fn scopes_display(scopes: &[Scope]) -> String {
    if scopes.is_empty() {
        return "all".into();
    }
    scopes
        .iter()
        .map(Scope::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

/// Validate/resolve a knob value: freeform needs an explicit non-empty value;
/// listed knobs need a listed value, or nothing when exactly one is allowed.
pub fn resolve_value(knob: &EnvKnob, value: Option<&str>) -> Result<String> {
    let given = value.unwrap_or("");
    if knob.freeform.is_some() {
        if given.is_empty() {
            return Err(Error::InvalidKnobValue(format!(
                "{}: value required",
                knob.id
            )));
        }
        return Ok(given.to_string());
    }
    if let Some(g) = value.filter(|g| !g.is_empty()) {
        if knob.values.iter().any(|v| v.value == g) {
            return Ok(g.to_string());
        }
        let allowed: Vec<&str> = knob.values.iter().map(|v| v.value).collect();
        return Err(Error::InvalidKnobValue(format!(
            "{}: {} not allowed; allowed: {}",
            knob.id,
            g,
            allowed.join(", ")
        )));
    }
    if knob.values.len() == 1 {
        return Ok(knob.values[0].value.to_string());
    }
    let allowed: Vec<&str> = knob.values.iter().map(|v| v.value).collect();
    Err(Error::InvalidKnobValue(format!(
        "{}: value required; allowed: {}",
        knob.id,
        allowed.join(", ")
    )))
}

/// Env-assign grammar, same rule as launch `KEY=VAL` tokens.
pub fn valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn validate_custom_key(key: &str) -> Result<()> {
    if valid_env_key(key) {
        Ok(())
    } else {
        Err(Error::InvalidEnvKey(key.into()))
    }
}
