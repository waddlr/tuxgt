use fluent::{FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

use crate::{Error, Result};

pub use fluent::FluentArgs;
pub const EN_US_FTL: &str = include_str!("../l10n/en-US/cli.ftl");

pub struct Strings {
    bundle: FluentBundle<FluentResource>,
}

impl Strings {
    pub fn en_us() -> Result<Self> {
        let res = FluentResource::try_new(EN_US_FTL.to_string())
            .map_err(|_| Error::Fluent("parse en-US catalog".into()))?;
        let loc: LanguageIdentifier = "en-US"
            .parse()
            .map_err(|_| Error::Fluent("parse en-US locale".into()))?;
        let mut bundle = FluentBundle::new(vec![loc]);
        // E41: keep `get_args` output byte-identical to the `format!` it
        // replaces. Fluent otherwise wraps every `{ $arg }` value in invisible
        // bidi isolates (U+2068/U+2069); the catalog is en-US only.
        bundle.set_use_isolating(false);
        bundle
            .add_resource(res)
            .map_err(|_| Error::Fluent("add en-US catalog".into()))?;
        Ok(Self { bundle })
    }

    pub fn get(&self, id: &str) -> String {
        self.get_args(id, None)
    }

    pub fn get_args(&self, id: &str, args: Option<&FluentArgs>) -> String {
        let Some(msg) = self.bundle.get_message(id) else {
            return id.to_string();
        };
        let Some(pattern) = msg.value() else {
            return id.to_string();
        };
        let mut errors = Vec::new();
        let s = self.bundle.format_pattern(pattern, args, &mut errors);
        s.to_string()
    }
}
