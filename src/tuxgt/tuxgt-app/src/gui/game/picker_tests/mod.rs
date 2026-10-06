use super::{ModIds, ModRow};

mod order;
mod parked;
mod picker;

fn row(instance: &str, mod_type: &str, installed: bool) -> ModRow {
    ModRow {
        instance: instance.into(),
        label: instance.into(),
        mod_type: mod_type.into(),
        official: false,
        adapter: "preload".into(),
        enabled: true,
        files: 1,
        load_order: 0,
        installed,
        slot: String::new(),
        slot_capable: false,
        graph: String::new(),
        file_entries: Box::default(),
        env_entries: Box::default(),
        effect_files: Box::default(),
        asset: None,
        payload_present: false,
        ids: ModIds::for_instance(instance, "test-game"),
    }
}
