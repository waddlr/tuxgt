//! Every `gui-*` id the GUI resolves on the main thread (B06b),
//! generated from `l10n/en-US/cli.ftl` at build time. The
//! `fluent_gui_catalog_resolves` test asserts none of them echo.
include!(concat!(env!("OUT_DIR"), "/gui_ids_generated.rs"));
