//! Label for the slot control. The click still sends `<self>` or a proxy
//! stem. The closed control and the menu share one claiming file. Diagnose
//! still uses [`super::load_mods::proxy_slot`].

use super::{ModFileRow, Shell};

/// A proxy dest shows its stem (`dxgi`). Anything else shows that dest's
/// filename (`ReShade64.dll`). `None` when there is no top-level DLL to name.
pub(crate) fn shown_slot(dest: Option<&str>) -> Option<String> {
    let dest = dest?;
    if let Ok(slot) = tuxgt_core::parse_slot(dest) {
        return Some(slot.as_str().to_string());
    }
    Some(dest.to_string())
}

/// Closed label on a slot-choice row. A `<self>` pick paints `stock`, or
/// `unknown` when that filename is not known. Never the token.
pub(crate) fn choice_label(slot: &str, stock: &str, unknown: &str) -> String {
    if tuxgt_core::is_self_slot(slot) {
        if !stock.is_empty() {
            stock.to_string()
        } else {
            plain_label(unknown)
        }
    } else {
        slot.to_string()
    }
}

fn plain_label(unknown: &str) -> String {
    if unknown.is_empty() || unknown.contains("<self>") {
        "DLL name".to_string()
    } else {
        unknown.to_string()
    }
}

/// Install warns when the shown choice is the DLL's own name.
pub(crate) fn install_stock_warning(adapter: &str, shown: &str) -> bool {
    tuxgt_core::is_install(adapter)
        && shown.to_ascii_lowercase().ends_with(".dll")
        && tuxgt_core::parse_slot(shown).is_err()
}

/// Menu rows: `(label, value)`. `value` is what core receives. The stock
/// row's label is the dest filename; its value stays `<self>`. `unknown`
/// is the label when that filename is not known.
pub(crate) fn slot_menu(stock: &str, unknown: &str) -> Vec<(String, &'static str)> {
    let self_label = if stock.is_empty() {
        plain_label(unknown)
    } else {
        stock.to_string()
    };
    let mut out = Vec::with_capacity(1 + Shell::ADD_SLOTS.len());
    out.push((self_label, tuxgt_core::SELF_SLOT));
    for stem in Shell::ADD_SLOTS {
        out.push((stem.to_string(), stem));
    }
    out
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn named_injector(name: &str) -> bool {
    name.eq_ignore_ascii_case("OptiScaler.dll")
        || name.eq_ignore_ascii_case("ReShade64.dll")
        || name.eq_ignore_ascii_case("ReShade32.dll")
}

fn claim_label(dest: &str, source: &str) -> String {
    let src = basename(source);
    if src.is_empty() {
        basename(dest).to_string()
    } else {
        src.to_string()
    }
}

fn claiming_row(files: &[ModFileRow]) -> Option<&ModFileRow> {
    let claiming: Vec<&ModFileRow> = files
        .iter()
        .filter(|f| {
            f.enabled
                && f.recipe_load
                && !f.dest.contains('/')
                && !f.dest.contains('\\')
                && !tuxgt_core::is_prefix_dest(&f.dest)
        })
        .collect();
    if let Some(f) = claiming
        .iter()
        .copied()
        .find(|f| tuxgt_core::parse_slot(basename(&f.dest)).is_ok())
    {
        return Some(f);
    }
    let injectors: Vec<&ModFileRow> = claiming
        .iter()
        .copied()
        .filter(|f| named_injector(basename(&f.dest)))
        .collect();
    if injectors.len() == 1 {
        return Some(injectors[0]);
    }
    (claiming.len() == 1).then(|| claiming[0])
}

/// Stock menu label for an installed row. A proxy dest (`dxgi.dll`) still
/// names the source basename, so the menu offers `ReShade64.dll`.
pub(crate) fn stock_menu_label(files: &[ModFileRow]) -> Option<String> {
    claiming_row(files).map(|f| claim_label(&f.dest, &f.source))
}

/// Closed Slot control. Same claiming file as [`stock_menu_label`]: a proxy
/// dest shows its stem, anything else the source basename. Claim follows
/// the recipe, not a per-game Load/Include switch.
pub(crate) fn closed_slot_label(
    files: &[tuxgt_core::PlannedFile],
    include: &[String],
) -> Option<String> {
    let rows: Vec<ModFileRow> = files
        .iter()
        .map(|f| ModFileRow {
            dest: f.dest.clone(),
            source: f.source.clone(),
            enabled: f.enabled,
            required: false,
            loaddll: tuxgt_core::is_dll(&f.dest) && !tuxgt_core::include_covers(include, &f.dest),
            recipe_load: tuxgt_core::is_dll(&f.dest)
                && !tuxgt_core::include_covers(include, &f.dest),
        })
        .collect();
    let f = claiming_row(&rows)?;
    let base = basename(&f.dest);
    if tuxgt_core::parse_slot(base).is_ok() {
        return shown_slot(Some(base));
    }
    Some(claim_label(&f.dest, &f.source))
}

/// Filename for a mod that is not installed yet. OptiScaler is always
/// `OptiScaler.dll`. ReShade follows the payload keep for this game's arch.
pub(crate) fn recipe_stock_label(
    mod_type: &str,
    rules: &[tuxgt_core::PayloadRule],
    arch: Option<&str>,
    api: Option<&str>,
) -> Option<String> {
    if mod_type == "optiscaler" {
        return Some("OptiScaler.dll".to_string());
    }
    let arch = arch.unwrap_or("");
    let api = api.unwrap_or("");
    let mut names = Vec::new();
    for rule in rules {
        if rule.arch.as_deref().is_some_and(|a| a != arch) {
            continue;
        }
        if rule.api.as_deref().is_some_and(|a| a != api) {
            continue;
        }
        for glob in rule.keep.iter() {
            if glob.contains('*') || glob.contains('?') {
                continue;
            }
            let base = basename(glob);
            if !base.to_ascii_lowercase().ends_with(".dll") {
                continue;
            }
            if names.iter().any(|n: &String| n.eq_ignore_ascii_case(base)) {
                continue;
            }
            names.push(base.to_string());
        }
    }
    let injectors: Vec<String> = names
        .iter()
        .filter(|n| named_injector(n))
        .cloned()
        .collect();
    if injectors.len() == 1 {
        return injectors.into_iter().next();
    }
    if names.len() == 1 {
        return names.into_iter().next();
    }
    None
}

/// Status text after a pick. `<self>` reports the filename core wrote.
/// `unknown` is used when that filename cannot be read. Never the token.
pub(crate) fn status_slot_name(
    requested: &str,
    files: &[tuxgt_core::PlannedFile],
    unknown: &str,
) -> String {
    if !tuxgt_core::is_self_slot(requested) {
        return requested.to_string();
    }
    let tops: Vec<&str> = files
        .iter()
        .filter(|f| {
            f.enabled
                && tuxgt_core::is_dll(&f.dest)
                && !f.dest.contains('/')
                && !f.dest.contains('\\')
                && !tuxgt_core::is_prefix_dest(&f.dest)
        })
        .map(|f| basename(&f.dest))
        .collect();
    let named: Vec<&str> = tops
        .iter()
        .copied()
        .filter(|d| named_injector(d) && tuxgt_core::parse_slot(d).is_err())
        .collect();
    if named.len() == 1 {
        return named[0].to_string();
    }
    let plain: Vec<&str> = tops
        .iter()
        .copied()
        .filter(|d| tuxgt_core::parse_slot(d).is_err())
        .collect();
    if plain.len() == 1 {
        return plain[0].to_string();
    }
    plain_label(unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(source: &str, dest: &str) -> ModFileRow {
        ModFileRow {
            dest: dest.into(),
            source: source.into(),
            enabled: true,
            required: false,
            loaddll: true,
            recipe_load: true,
        }
    }

    fn planned(dest: &str) -> tuxgt_core::PlannedFile {
        tuxgt_core::PlannedFile {
            source: format!("mods/official/reshade/{dest}"),
            dest: dest.into(),
            sha256: String::new(),
            enabled: true,
            load: None,
        }
    }

    #[test]
    fn stem_or_filename() {
        assert_eq!(shown_slot(Some("dxgi.dll")).as_deref(), Some("dxgi"));
        assert_eq!(
            shown_slot(Some("ReShade64.dll")).as_deref(),
            Some("ReShade64.dll")
        );
        assert_eq!(shown_slot(None), None);
    }

    #[test]
    fn proxy_dest_still_offers_the_source_name() {
        let files = [
            file("mods/official/reshade/ReShade64.dll", "dxgi.dll"),
            file("mods/official/reshade/ReShade64.json", "ReShade64.json"),
        ];
        assert_eq!(stock_menu_label(&files).as_deref(), Some("ReShade64.dll"));
        let files = [
            file("mods/official/optiscaler/OptiScaler.dll", "OptiScaler.dll"),
            file(
                "mods/official/optiscaler/amd_fidelityfx_dx12.dll",
                "amd_fidelityfx_dx12.dll",
            ),
        ];
        assert_eq!(stock_menu_label(&files).as_deref(), Some("OptiScaler.dll"));
    }

    #[test]
    fn include_switch_keeps_the_injector_as_the_slot_claim() {
        let mut injector = file("mods/official/reshade/ReShade64.dll", "ReShade64.dll");
        injector.loaddll = false;
        let companion = file("mods/official/reshade/companion.dll", "companion.dll");
        assert_eq!(
            stock_menu_label(&[injector, companion]).as_deref(),
            Some("ReShade64.dll")
        );
    }

    #[test]
    fn recipe_names_follow_arch() {
        let rules = [
            tuxgt_core::PayloadRule {
                arch: Some("64".into()),
                api: None,
                keep: Box::new(["ReShade64.dll".into()]),
                drop: Box::new([]),
            },
            tuxgt_core::PayloadRule {
                arch: Some("32".into()),
                api: None,
                keep: Box::new(["ReShade32.dll".into()]),
                drop: Box::new([]),
            },
        ];
        assert_eq!(
            recipe_stock_label("reshade", &rules, Some("64"), None).as_deref(),
            Some("ReShade64.dll")
        );
        assert_eq!(
            recipe_stock_label("reshade", &rules, Some("32"), None).as_deref(),
            Some("ReShade32.dll")
        );
        assert_eq!(recipe_stock_label("reshade", &rules, None, None), None);
        assert_eq!(
            recipe_stock_label("optiscaler", &[], None, None).as_deref(),
            Some("OptiScaler.dll")
        );
    }

    fn planned_src(source: &str, dest: &str) -> tuxgt_core::PlannedFile {
        tuxgt_core::PlannedFile {
            source: source.into(),
            dest: dest.into(),
            sha256: String::new(),
            enabled: true,
            load: None,
        }
    }

    #[test]
    fn menu_label_is_not_the_token() {
        let menu = slot_menu("ReShade64.dll", "DLL name");
        assert_eq!(menu[0].0, "ReShade64.dll");
        assert_eq!(menu[0].1, "<self>");
        assert_eq!(menu[1].1, "dxgi");
        assert_eq!(menu[2].1, "d3d9");
        assert_eq!(menu[3].1, "d3d10");
        let unknown = slot_menu("", "DLL name");
        assert_eq!(unknown[0].0, "DLL name");
        assert_eq!(unknown[0].1, "<self>");
        assert!(!unknown[0].0.contains("<self>"));
        assert_eq!(
            choice_label("<self>", "ReShade64.dll", "DLL name"),
            "ReShade64.dll"
        );
        assert_eq!(choice_label("<self>", "", "DLL name"), "DLL name");
        assert_eq!(choice_label("dxgi", "ReShade64.dll", "DLL name"), "dxgi");
        assert!(install_stock_warning("install", "ReShade64.dll"));
        assert!(!install_stock_warning("install", "dxgi"));
        assert!(!install_stock_warning("preload", "ReShade64.dll"));
        assert_eq!(
            status_slot_name("<self>", &[planned("ReShade64.dll")], "DLL name"),
            "ReShade64.dll"
        );
        assert_eq!(
            status_slot_name("dxgi", &[planned("dxgi.dll")], "DLL name"),
            "dxgi"
        );
        assert_eq!(status_slot_name("<self>", &[], "DLL name"), "DLL name");
        assert!(!status_slot_name("<self>", &[], "<self>").contains("<self>"));
    }

    #[test]
    fn closed_control_follows_the_claiming_injector() {
        let stock = [
            planned_src("amd_fidelityfx_dx12.dll", "amd_fidelityfx_dx12.dll"),
            planned_src("OptiScaler.dll", "OptiScaler.dll"),
        ];
        assert_eq!(
            closed_slot_label(&stock, &[]).as_deref(),
            Some("OptiScaler.dll")
        );
        let proxied = [
            planned_src("amd_fidelityfx_dx12.dll", "amd_fidelityfx_dx12.dll"),
            planned_src("OptiScaler.dll", "dxgi.dll"),
        ];
        assert_eq!(closed_slot_label(&proxied, &[]).as_deref(), Some("dxgi"));
    }
}
