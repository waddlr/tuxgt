use super::*;
use tuxgt_core::{
    family_template, list_family_assets_many, list_reshade_packages, list_templates,
    migrate_reshade_legacy, mint_recipe, snap_family_asset, MintTarget, RecipeSpec,
    ReshadePackageKind,
};

pub(crate) async fn run(pool: &SqlitePool, cmd: MintCmd) -> CliResult {
    match cmd {
        MintCmd::ListFamily => list_family().await,
        MintCmd::ListReshade => list_reshade().await,
        MintCmd::Family {
            template,
            asset,
            game,
        } => mint_family(pool, &template, &asset, &game).await,
        MintCmd::Reshade {
            package,
            game,
            kind,
        } => mint_reshade(pool, &package, &game, kind.as_deref()).await,
    }
}

async fn list_family() -> CliResult {
    let data = data_dir();
    let ids: Vec<String> = list_templates(&data)?
        .into_iter()
        .filter(|t| t.family.is_some())
        .map(|t| t.id)
        .collect();
    for (tid, listed) in list_family_assets_many(&data, &ids).await {
        match listed {
            Ok(assets) => {
                for a in assets {
                    println!("{tid}\t{}\t{}", a.name, a.tag);
                }
            }
            Err(e) => eprintln!("broken\t{tid}\t{e}"),
        }
    }
    Ok(())
}

async fn list_reshade() -> CliResult {
    let pkgs = list_reshade_packages(&config_dir(), &data_dir()).await?;
    for p in pkgs {
        let arch = match (p.url.is_some(), p.url32.is_some()) {
            (true, true) => "both",
            (true, false) => "64",
            (false, true) => "32",
            (false, false) => "none",
        };
        println!(
            "{}\t{}\t{arch}\t{}\t{}",
            p.kind.as_str(),
            p.name,
            if p.in_catalog { "catalog64" } else { "" },
            if p.in_catalog_32 { "catalog32" } else { "" }
        );
    }
    Ok(())
}

async fn mint_family(pool: &SqlitePool, template: &str, asset: &str, game: &str) -> CliResult {
    let row = game_row(pool, game).await?;
    let display = row
        .name
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(game)
        .to_string();
    let appid = row
        .resolved_appid()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|a| *a != 0);
    let data = data_dir();
    let (tpl, fam) = family_template(&data, template)?;
    let snapped = snap_family_asset(&data, template, asset).await;
    let label = family_label(&tpl.label, &snapped);
    let spec = RecipeSpec::family(&tpl, &fam, &snapped, &display, &label, appid)?;
    let m = mint_recipe(&config_dir(), &data, spec)?;
    println!("{}\t{}\t{}", m.id, m.mod_type, m.label);
    tracing::info!(source = m.id.as_str(), "mod minted");
    Ok(())
}

fn family_label(family_label: &str, asset: &str) -> String {
    let stem = std::path::Path::new(asset)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| asset.to_string());
    let prefix = format!("{}-", family_label.to_lowercase());
    let title = stem
        .to_ascii_lowercase()
        .strip_prefix(&prefix)
        .map(str::to_string)
        .unwrap_or_else(|| stem.to_ascii_lowercase())
        .replace(['-', '_'], " ");
    format!("{family_label}: {}", title.trim())
}

async fn mint_reshade(
    pool: &SqlitePool,
    package: &str,
    game: &str,
    kind: Option<&str>,
) -> CliResult {
    let row = game_row(pool, game).await?;
    let arch = row
        .bitness
        .as_deref()
        .filter(|b| *b == "32" || *b == "64")
        .ok_or("game has no 32/64 bitness; set it with tuxgt doctor --set bitness=64")?;
    let pkgs = list_reshade_packages(&config_dir(), &data_dir()).await?;
    if let Some(k) = kind {
        if k != ReshadePackageKind::Effect.as_str() && k != ReshadePackageKind::Addon.as_str() {
            return Err("--kind must be effect or addon".into());
        }
    }
    let matches: Vec<_> = pkgs
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(package))
        .filter(|p| kind.is_none_or(|k| p.kind.as_str() == k))
        .collect();
    let pkg = match matches.as_slice() {
        [one] => (*one).clone(),
        [] => {
            return Err(format!("unknown ReShade package: {package}").into());
        }
        _ => {
            return Err(format!(
                "package {package} is listed as both effect and addon; pass --kind effect|addon"
            )
            .into());
        }
    };
    if !pkg.mintable_for_arch(arch) {
        return Err(format!("{}: not mintable for {arch}-bit", pkg.name).into());
    }
    let cfg = config_dir();
    let data = data_dir();
    let migrated = migrate_reshade_legacy(&cfg, &data, &pkg)?;
    // The 64-bit rename *is* the mint; minting again refuses already-in-catalog.
    if let Some(id) = migrated.as_ref().filter(|_| arch != "32") {
        if let Ok(m) = tuxgt_core::find_mod(&cfg, &data, id) {
            println!("{}\t{}\t{}", m.id, m.mod_type, m.label);
        } else {
            println!("{id}");
        }
        tracing::info!(source = id.as_str(), "mod minted");
        return Ok(());
    }
    let target = MintTarget {
        appid: row
            .resolved_appid()
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|a| *a != 0),
        title: row
            .name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(game)
            .to_string(),
    };
    let spec = RecipeSpec::reshade_package_for_game(&pkg, arch, &target)?;
    let m = mint_recipe(&cfg, &data, spec)?;
    println!("{}\t{}\t{}", m.id, m.mod_type, m.label);
    tracing::info!(source = m.id.as_str(), "mod minted");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::family_label;

    #[test]
    fn family_label_strips_vendor_prefix() {
        assert_eq!(
            family_label("Luma", "Luma-NieR_Automata.zip"),
            "Luma: nier automata"
        );
        assert_eq!(
            family_label("RenoDX", "renodx-cp2077.addon64"),
            "RenoDX: cp2077"
        );
    }
}
