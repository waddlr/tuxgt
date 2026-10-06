use super::super::super::{
    ClientStopOp, ConfirmOp, PendingConfirm, ReqDep, SlotChoiceOp, SlotPick,
};
use super::super::install_work_parked;

/// R12: a Hide must be refused while work that lives only in this window
/// is unfinished — the tail of a batch, or the install-family confirm it
/// parked behind. Every install/update confirm counts; a Host or
/// ClientStop confirm owns no batch, and a single-row op has no tail.
#[test]
fn parked_install_work_blocks_a_hide() {
    let overwrite = |op| {
        Some(PendingConfirm::Overwrite {
            game: "quake".into(),
            instance: "reshade".into(),
            op,
            dests: Box::new(["game/dir".to_string()]),
        })
    };
    let requires = Some(PendingConfirm::Requires {
        game: "quake".into(),
        instance: "reshade".into(),
        deps: Box::new([ReqDep {
            req: "d3dcompiler-47".into(),
            candidates: Box::new([("d3dcompiler-47".to_string(), "d3dcompiler-47".into())]),
            chosen: None,
        }]),
    });
    let queue: Vec<String> = Vec::new();
    let batch: Vec<String> = vec!["my-addon".into()];

    // Nothing parked: the veto belongs to the transfer alone, which has
    // already released it.
    assert!(!install_work_parked(&queue, None));
    // A queued tail is unstarted work a Hide would drop.
    assert!(install_work_parked(&batch, None));
    // The install-family confirms a transfer can park.
    assert!(install_work_parked(&queue, requires.as_ref()));
    for (label, op) in [
        (
            "install",
            ConfirmOp::Install {
                with_requires: None,
                slot: None,
            },
        ),
        (
            "update",
            ConfirmOp::Update {
                adapter: Some("preload".into()),
                slot: None,
            },
        ),
        (
            "update-force",
            ConfirmOp::UpdateForce {
                adapter: Some("preload".into()),
                foreign_done: false,
                slot: None,
            },
        ),
    ] {
        assert!(
            install_work_parked(&queue, overwrite(op).as_ref()),
            "{label} parks install work"
        );
    }
    // Slot-choice cards park the same veto for install-ish ops; the
    // game-scoped conversion owns no batch.
    let slot_choice = |op| {
        Some(PendingConfirm::SlotChoice {
            game: "quake".into(),
            op,
            picks: Box::new([SlotPick {
                instance: "reshade".into(),
                slot: "dxgi".into(),
                stock: "ReShade64.dll".into(),
            }]),
        })
    };
    for (label, op) in [
        (
            "slot-install",
            SlotChoiceOp::Install {
                target: "reshade".into(),
                with_requires: None,
            },
        ),
        (
            "slot-update",
            SlotChoiceOp::Update {
                adapter: Some("preload".into()),
            },
        ),
        (
            "slot-update-force",
            SlotChoiceOp::UpdateForce {
                adapter: Some("preload".into()),
                foreign_done: false,
            },
        ),
    ] {
        assert!(
            install_work_parked(&queue, slot_choice(op).as_ref()),
            "{label} parks install work"
        );
    }
    assert!(
        !install_work_parked(
            &queue,
            slot_choice(SlotChoiceOp::AdapterConvert {
                adapter: "install".into(),
                unhook: false,
            })
            .as_ref()
        ),
        "slot-convert parks no batch"
    );
    // Confirms that own no batch must not refuse Close forever.
    for (label, op) in [
        ("uninstall", ConfirmOp::Uninstall),
        ("enable", ConfirmOp::Enable { on: true }),
        ("slot", ConfirmOp::Slot { slot: "1".into() }),
        (
            "file-keep",
            ConfirmOp::FileKeep {
                dest: "dxgi.dll".into(),
                on: true,
            },
        ),
    ] {
        assert!(
            !install_work_parked(&queue, overwrite(op).as_ref()),
            "{label} parks no batch"
        );
    }
    assert!(!install_work_parked(
        &queue,
        Some(&PendingConfirm::Host {
            op: ConfirmOp::HostInstall,
            paths: Box::new(["bin".to_string()]),
        })
    ));
    assert!(!install_work_parked(
        &queue,
        Some(&PendingConfirm::ClientStop {
            game: "quake".into(),
            op: ClientStopOp::ApplyMode,
        })
    ));
    // A batch with a non-install confirm parked still holds the veto.
    assert!(install_work_parked(
        &batch,
        Some(&PendingConfirm::ClientStop {
            game: "quake".into(),
            op: ClientStopOp::ApplyMode,
        })
    ));
}
