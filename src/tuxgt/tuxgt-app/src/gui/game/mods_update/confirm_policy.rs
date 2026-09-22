use super::super::{confirm_dests, ConfirmOp};

/// How a per-game Update parks a `NeedConfirm`.
///
/// A `config-overwrite` marker is not a card: the redownload replaces
/// depot bytes, and an unforced reinstall keeps per-game staged touches.
/// Foreign game-dir messages still park. `None` means show no card.
pub(crate) fn confirm_op_for_update(
    msg: &str,
    force_staging: bool,
    adapter: Option<String>,
    slot: Option<String>,
) -> Option<(ConfirmOp, Vec<String>)> {
    if msg.starts_with("config-overwrite:") {
        return None;
    }
    let dests = confirm_dests(msg);
    let op = if force_staging {
        ConfirmOp::UpdateForce {
            adapter,
            foreign_done: true,
            slot,
        }
    } else {
        ConfirmOp::Update { adapter, slot }
    };
    Some((op, dests))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_edits_do_not_park_a_confirm() {
        assert!(
            confirm_op_for_update("config-overwrite: Mod.ini, User.cfg", false, None, None,)
                .is_none()
        );
        assert!(confirm_op_for_update("config-overwrite:", true, None, None).is_none());
    }

    #[test]
    fn foreign_dests_still_park() {
        let (op, dests) = confirm_op_for_update(
            "foreign game-dir dests: dxgi.dll, ReShade64.dll",
            false,
            Some("install".into()),
            Some("dxgi".into()),
        )
        .expect("foreign confirm");
        assert!(matches!(
            op,
            ConfirmOp::Update {
                adapter: Some(ref a),
                slot: Some(ref s),
            } if a == "install" && s == "dxgi"
        ));
        assert_eq!(dests, ["dxgi.dll", "ReShade64.dll"]);
    }

    #[test]
    fn forced_foreign_keeps_the_wipe() {
        let (op, dests) =
            confirm_op_for_update("foreign game-dir dests: dxgi.dll", true, None, None)
                .expect("forced foreign");
        assert!(matches!(
            op,
            ConfirmOp::UpdateForce {
                foreign_done: true,
                adapter: None,
                slot: None,
            }
        ));
        assert_eq!(dests, ["dxgi.dll"]);
    }
}
