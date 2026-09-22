use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) static TEST_N: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_config() -> PathBuf {
    let n = TEST_N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tuxgt-e12-{}-{}", std::process::id(), n));
    let _ = fs::remove_dir_all(&dir);
    dir
}

pub(crate) fn fake(
    name: &'static str,
    label_id: &'static str,
    requires: Option<&'static [&'static str]>,
) -> PluginDesc {
    PluginDesc {
        registry: None,
        name,
        tag: None,
        label_id,
        requires,
    }
}
