//! Offline protonfixes annotation and the ProtonDB schema spike.

use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(cmd) = args.first().map(String::as_str) else {
        return usage();
    };
    match cmd {
        "protonfixes" => protonfixes(&args[1..]),
        "protondb" => protondb(&args[1..]),
        _ => usage(),
    }
}

fn protonfixes(args: &[String]) -> ExitCode {
    let Some(pin) = flag(args, "--pin") else {
        return usage();
    };
    let Some(src) = flag(args, "--src") else {
        return usage();
    };
    let Some(out) = flag(args, "--out") else {
        return usage();
    };
    match tuxgt_core::harvest_protonfixes(Path::new(src), Path::new(out), pin) {
        Ok(n) => {
            println!("{n}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn protondb(args: &[String]) -> ExitCode {
    let Some(dump) = flag(args, "--dump") else {
        return usage();
    };
    match tuxgt_core::protondb_has_launch_field(Path::new(dump)) {
        Ok(false) => {
            eprintln!("pinned dump has no structured launch field; stopping");
            ExitCode::from(2)
        }
        Ok(true) => {
            println!("v1 does not emit ProtonDB catalog files");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: args-harvest protonfixes --pin <commit> --src <checkout> --out <dir>\n       args-harvest protondb --dump <file>"
    );
    ExitCode::from(2)
}
