pub(crate) use std::collections::{HashMap, HashSet};
pub(crate) use std::path::PathBuf;

pub(crate) use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
pub(crate) use gpui_kit::component::{Root, Theme, TitleBar, VirtualListScrollHandle};
pub(crate) use gpui_kit::*;

pub(crate) use tuxgt_core::{
    data_dir, has_apply_record, host_gpu, load_host_inventory, verify_host_install, Strings,
};

pub(crate) use super::notice_store::NoticeStore;
pub(crate) use super::prefs::Prefs;
pub(crate) use super::sys::SysInfo;

mod assets;
mod run;

pub(crate) use assets::{icon_file, Assets};
pub use run::run;
