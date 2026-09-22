mod hook;
mod install;
mod types;
mod uninstall;
mod verify;

pub(crate) use hook::*;
pub(crate) use install::*;
pub(crate) use types::*;
pub(crate) use verify::*;

pub use hook::{install_proton_hook, uninstall_proton_hook};
pub use install::{
    expand_tilde, icon_path, install_userland, install_userland_with_home, packaged_prefix,
    InstallReport, ICON_SIZES,
};
pub use types::{
    is_icon_host_path, HostInstallInventory, HostInventoryEntry, HostKind, HostStatus,
    HostVerifyEntry, IntendedHostEntry,
};
pub use uninstall::{uninstall_userland, UninstallReport};
pub use verify::{
    collapse_icon_paths, host_inventory_from_intended, icons_check_line, install_check_lines,
    install_check_ok, install_check_summary, intended_host_manifest, is_required_host_path,
    load_host_inventory, required_host_failures, required_host_warning, save_host_inventory,
    verify_host_install, RequiredHostWarning,
};

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
#[cfg(test)]
mod tests_2;
