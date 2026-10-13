mod claims;
mod conflicts;
mod copies;
mod material;
mod prefix;
mod promote;

pub(crate) use claims::*;
pub(crate) use copies::*;
pub(crate) use material::{
    backup_target, backup_target_bytes, classify_dest, remove_owned_dest, DestStatus,
};
pub(crate) use prefix::*;
pub(crate) use promote::*;

pub use conflicts::{load_conflicts, need_manifest, other_claims, LoadConflict};
pub use copies::{
    apply_copies, foreign_occupied, game_root, game_roots, plan_copies, plan_dest_copy,
    remove_copies, remove_dest_copy, CopyOp, Removal,
};
pub use material::InstallMaterial;
pub use prefix::{
    foreign_dest, is_prefix_dest, prefix_drive_c, prefix_for, prefix_rel, prefix_root,
    validate_prefix_dests, PREFIX_DEST_PREFIX,
};

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
#[cfg(test)]
mod tests_game_root;
#[cfg(test)]
mod tests_links;
