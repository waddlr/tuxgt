use std::collections::HashMap;

use super::super::{
    install_spawn_apply, make_win_order, merge_install_queue, move_in_order, move_to_edge,
    order_install_ids,
};
use super::row;

/// Moves are section-local, and a non-official row never swaps past the
/// pinned official above it (nor an official past a user row).
#[test]
fn move_in_order_stays_in_section_and_kind() {
    let mut reshade = row("reshade", "reshade", true);
    reshade.official = true;
    let mut fx = row("my-effect", "effect", true);
    fx.load_order = 1;
    let rows = vec![reshade, fx];
    assert_eq!(move_in_order(&rows, "my-effect", -1), None);
    let mut a = row("a", "effect", true);
    a.load_order = 0;
    let mut b = row("b", "effect", true);
    b.load_order = 1;
    let rows = vec![a, b];
    assert_eq!(move_in_order(&rows, "b", -1).unwrap(), ["b", "a"]);
    assert_eq!(move_in_order(&rows, "a", -1), None);
    assert_eq!(move_in_order(&rows, "b", 1), None);
    assert_eq!(move_in_order(&rows, "ghost", 1), None);
}

/// Make-win moves a rival inside its own section only: the id set is
/// unchanged (core rejects anything else) and other sections keep order.
#[test]
fn make_win_orders_group_last_inside_section() {
    let mut a = row("a", "effect", true);
    a.load_order = 0;
    let mut b = row("b", "effect", true);
    b.load_order = 1;
    let mut c = row("c", "texture", true);
    c.load_order = 2;
    let rows = vec![a, b, c];
    assert_eq!(
        make_win_order(&rows, "a", &["a".to_string(), "b".to_string()]).unwrap(),
        ["b", "a", "c"]
    );
    assert_eq!(make_win_order(&rows, "ghost", &["a".to_string()]), None);
}

/// Edge moves jump to the end of the card's same-officialness run
/// inside its section: a user card never passes the pinned official
/// above it, and sitting on the edge is a no-op.
#[test]
fn move_to_edge_orders_run_edges() {
    let mut reshade = row("reshade", "reshade", true);
    reshade.official = true;
    let mut a = row("a", "effect", true);
    a.load_order = 1;
    let mut b = row("b", "effect", true);
    b.load_order = 2;
    let rows = vec![reshade, a, b];
    assert_eq!(move_to_edge(&rows, "a", true), None);
    assert_eq!(
        move_to_edge(&rows, "b", true).unwrap(),
        ["reshade", "b", "a"]
    );
    assert_eq!(
        move_to_edge(&rows, "a", false).unwrap(),
        ["reshade", "b", "a"]
    );
    assert_eq!(move_to_edge(&rows, "b", false), None);
    assert_eq!(move_to_edge(&rows, "reshade", false), None);
    assert_eq!(move_to_edge(&rows, "ghost", true), None);
    // Officials form their own run: o1 sinks below o2 but never past u1.
    let mut o1 = row("o1", "reshade", true);
    o1.official = true;
    let mut o2 = row("o2", "effect", true);
    o2.official = true;
    o2.load_order = 1;
    let mut u1 = row("u1", "effect", true);
    u1.load_order = 2;
    let rows = vec![o1, o2, u1];
    assert_eq!(move_to_edge(&rows, "o1", true), None);
    assert_eq!(
        move_to_edge(&rows, "o1", false).unwrap(),
        ["o2", "o1", "u1"]
    );
    assert_eq!(move_to_edge(&rows, "o2", true).unwrap(), ["o2", "o1", "u1"]);
}

/// R70: a picker Install while a spawn is in flight appends; it does
/// not replace the queue or ask the caller to start another spawn.
#[test]
fn merge_install_queue_appends_while_busy() {
    let info = HashMap::new();
    let mut q = vec!["old".into()];
    assert!(merge_install_queue(
        &mut q,
        None,
        &["my-effect".into(), "d3dcompiler-47".into()],
        &info
    ));
    assert_eq!(q, ["my-effect", "d3dcompiler-47"]);

    let mut q = vec!["my-addon".into()];
    assert!(!merge_install_queue(
        &mut q,
        Some("d3dcompiler-47"),
        &[
            "d3dcompiler-47".into(),
            "my-addon".into(),
            "my-effect".into()
        ],
        &info
    ));
    assert_eq!(q, ["my-addon", "my-effect"]);
}

/// R71: my-effect before ReShade in check order still installs ReShade
/// first when both are in the batch. Unchecked requires are not added.
#[test]
fn order_install_ids_selected_requires_first() {
    let mut info = HashMap::new();
    info.insert("my-effect".into(), ("effect".into(), Vec::new()));
    info.insert("reshade".into(), ("reshade".into(), Vec::new()));
    info.insert("my-addon".into(), ("reshade_addon".into(), Vec::new()));
    info.insert("d3dcompiler-47".into(), ("custom".into(), Vec::new()));
    let ids = vec![
        "my-effect".into(),
        "d3dcompiler-47".into(),
        "reshade".into(),
        "my-addon".into(),
    ];
    assert_eq!(
        order_install_ids(&ids, &info),
        ["d3dcompiler-47", "reshade", "my-effect", "my-addon"]
    );
    let only_fx = vec!["my-effect".into()];
    assert_eq!(order_install_ids(&only_fx, &info), ["my-effect"]);
    let mut info = HashMap::new();
    info.insert(
        "addon".into(),
        ("custom".into(), vec!["d3dcompiler-47".into()]),
    );
    info.insert("d3dcompiler-47".into(), ("custom".into(), Vec::new()));
    assert_eq!(
        order_install_ids(&["addon".into(), "d3dcompiler-47".into()], &info),
        ["d3dcompiler-47", "addon"]
    );
}

#[test]
fn install_spawn_apply_after_switch_back() {
    assert_eq!(install_spawn_apply(true, true, false), Some(true));
    assert_eq!(install_spawn_apply(false, true, false), Some(false));
    assert_eq!(install_spawn_apply(false, true, true), None);
    assert_eq!(install_spawn_apply(false, false, false), None);
    assert_eq!(install_spawn_apply(true, false, false), None);
}
