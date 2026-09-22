use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use super::*;

pub(crate) fn run(prefix: Option<PathBuf>, yes: bool, check: bool) -> CliResult {
    if check {
        return run_install_check(prefix);
    }
    run_install(prefix, yes)
}

pub(crate) fn uninstall(yes: bool) -> CliResult {
    // Host-only uninstall from the last install inventory; PREFIX stays.
    // Missing inventory errors (run `tuxgt install` once first); the
    // delete set is never guessed.
    let prefix = data_dir();
    let pending: Vec<String> = load_host_inventory(&prefix)?
        .map(|inv| {
            let paths: Vec<std::path::PathBuf> = inv.paths.iter().map(|e| e.path.clone()).collect();
            let (rest, icons) = collapse_icon_paths(&paths);
            let mut out: Vec<String> = rest.iter().map(|p| p.display().to_string()).collect();
            if let Some(line) = icons {
                out.push(line);
            }
            out
        })
        .unwrap_or_default();
    confirm_list("remove host files", &pending, yes)?;
    let rep = uninstall_userland(&prefix)?;
    let (removed, removed_icons) = collapse_icon_paths(&rep.removed);
    let (skipped, skipped_icons) = collapse_icon_paths(&rep.skipped);
    for p in &removed {
        println!("removed\t{}", p.display());
    }
    if let Some(line) = removed_icons {
        println!("removed\t{line}");
    }
    for p in &skipped {
        println!("skipped\t{}", p.display());
    }
    if let Some(line) = skipped_icons {
        println!("skipped\t{line}");
    }
    tracing::info!(removed = removed.len(), skipped = skipped.len(), "uninstalled");
    Ok(())
}

fn run_install_check(prefix: Option<PathBuf>) -> CliResult {
    // Verify-only: never prompts, never writes. `--prefix` overrides the
    // boot prefix; otherwise check what this process would boot from.
    let dest = prefix.as_deref().map(expand_tilde).unwrap_or_else(data_dir);
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is not set")?;
    let report = verify_host_install(&dest, &home);
    for line in install_check_lines(&report) {
        println!("{line}");
    }
    println!("{}", install_check_summary(&report));
    if install_check_ok(&report) {
        Ok(())
    } else {
        Err("host install drift: intended host files missing or modified".into())
    }
}

fn run_install(prefix: Option<PathBuf>, yes: bool) -> CliResult {
    let src = packaged_prefix()?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is not set")?;
    let default = home.join("tuxgt");
    let dest = match prefix {
        Some(p) => expand_tilde(&p),
        None if io::stdin().is_terminal() && !yes => {
            print!("Install prefix [{}]: ", default.display());
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
            let line = line.trim();
            if line.is_empty() {
                default.clone()
            } else {
                expand_tilde(&PathBuf::from(line))
            }
        }
        None => default.clone(),
    };
    let rep = install_userland(&src, &dest)?;
    let p = &rep.prefix;
    println!("PREFIX\t{}", p.display());
    println!("bin\t{}/bin/tuxgt", p.display());
    println!("wrapper\t{}/bin/tuxgt-launcher", p.display());
    println!("so\t{}/lib/libtuxgt-launcher.so", p.display());
    println!("mods\t{}/mods/", p.display());
    println!("games\t{}/games/", p.display());
    println!("downloads\t{}/downloads/", p.display());
    println!("config\t{}/config/", p.display());
    println!("sqlite\t{}/config/tuxgt.sqlite", p.display());
    for (link, target) in &rep.bin_links {
        println!("link\t{} -> {}", link.display(), target.display());
    }
    println!(
        "desktop\t{} -> {}",
        rep.desktop_link.display(),
        rep.desktop_target.display()
    );
    println!("icons\t{}/{}", rep.icons.0, rep.icons.1);
    println!("conf\t{}", rep.conf.display());
    println!("hooks\t{}/share/protonfixes/", p.display());
    for hook in &rep.hooks {
        println!("hook\t{}", hook.display());
    }
    if rep.moved {
        println!("moved\t{} -> {}", src.display(), p.display());
    }
    println!(
        "to relocate: mv {} <NEW> && <NEW>/bin/tuxgt install",
        p.display()
    );
    tracing::info!(prefix = p.display().to_string().as_str(), "installed");
    Ok(())
}
