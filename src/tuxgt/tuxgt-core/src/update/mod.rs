//! Self-update: own version vs the latest `waddlr/tuxgt` GitHub release,
//! and the manifest-tracked PREFIX overlay that applies it.

mod apply;
mod check;
mod manifest;
mod version;

pub use apply::{apply_app_update, AppUpdateReport};
pub use check::{app_version, check_app_update, AppUpdateStatus};
pub use manifest::{load_prefix_manifest, save_prefix_manifest, PrefixFile, PrefixManifest};
pub use version::{parse_app_version, AppVersion};

/// Release feed: the repo `make release` drafts to.
pub const UPDATE_OWNER: &str = "waddlr";
pub const UPDATE_REPO: &str = "tuxgt";
/// Release asset `make release` uploads (`dist/tuxgt.tar.gz`).
pub const UPDATE_ASSET: &str = "tuxgt.tar.gz";

#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
