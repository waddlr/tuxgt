mod conflicts;
mod copies;
mod prefix;

pub(crate) use copies::*;
pub(crate) use prefix::*;

pub use conflicts::{load_conflicts, need_manifest, other_claims, LoadConflict};
pub use copies::{
    apply_copies, foreign_occupied, game_root, plan_copies, plan_dest_copy, remove_copies,
    remove_dest_copy, tracked_dests, CopyOp, Removal,
};
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
