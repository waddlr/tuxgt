mod acquire;
mod artwork;
mod fetch;
mod github;
mod harvest;
mod hash;
mod manifest;
mod packages;
mod unpack;

pub(crate) use acquire::*;
pub(crate) use fetch::*;
pub(crate) use github::*;
pub(crate) use hash::*;
pub(crate) use manifest::*;
pub(crate) use packages::*;
pub(crate) use unpack::*;

pub use acquire::{acquire_with_source, cache_local};
pub use artwork::{
    art_fingerprint_file, bust_game_art_for_appid, render_game_art, thumb_file, touch_game_art,
    ArtKind,
};
pub use fetch::{fetch_url, CachedAsset, FetchProgress, ProgressSink};
pub use github::{list_family_assets, list_family_assets_many, FamilyAsset};
pub use harvest::{harvest_all, harvest_game, harvest_game_roots, harvest_roots};
pub use hash::{
    art_file, cache_dir, cached_file, drop_download, drop_game_art, drop_game_hero, drop_game_icon,
    drop_spent_downloads, game_manifests, hero_file, icon_file, land_art, land_hero, land_icon,
    manifest_path, sha256_file, sha256_hex, tmp_unpack_dir,
};
pub use manifest::{
    generated_globs_for, include_covers, is_required_dest, read_manifest, set_file_enabled,
    set_manifest_enabled, set_mod_env_enabled, write_manifest, FileManifest, ModProvenance,
    PlannedEnv, PlannedFile,
};
pub use packages::{
    cap_preview, list_reshade_packages, preview_effect_files, ReshadePackage, ReshadePackageKind,
};
pub use unpack::{
    all_tools, tool_status, unpack, unpack_with_password, ExtTool, ToolStatus, EXT_TOOLS,
};

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
#[cfg(test)]
mod tests_2;
