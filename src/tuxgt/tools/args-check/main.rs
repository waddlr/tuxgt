//! Fail the catalog on a schema or same-rank stem error.

use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let root = args.next().unwrap_or_else(|| "args".into());
    if args.next().is_some() {
        eprintln!("usage: args-check [dir]");
        return ExitCode::from(2);
    }
    match tuxgt_core::check_args_tree(Path::new(&root)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
