use super::super::*;

pub(crate) async fn install_mod_cli(
    pool: &tuxgt_core::SqlitePool,
    game: &str,
    instance_id: &str,
    adapter: Option<&str>,
    redownload: bool,
    with_requires: Option<String>,
    yes: bool,
    force: bool,
    password: Option<String>,
    slot: Option<String>,
) -> Result<FileManifest, Box<dyn std::error::Error>> {
    let password_from_flag = password.is_some();
    let mut opts = InstallOpts {
        adapter: adapter.map(str::to_string),
        redownload,
        with_requires,
        yes,
        force,
        password,
        slot,
    };
    loop {
        match install_instance(
            pool,
            &data_dir(),
            &config_dir(),
            game,
            instance_id,
            &opts,
            None,
        )
        .await
        {
            Ok(m) => {
                note_staging(&data_dir(), game);
                return Ok(m);
            }
            Err(Error::MissingRequires(t)) => {
                return Err(
                    format!("missing requires {t}; pass --with-requires <instance>").into(),
                );
            }
            Err(Error::NeedSlotChoice(msg)) => {
                let who = msg.split_once(':').map(|(_, r)| r.trim()).unwrap_or(&msg);
                // `--slot` serves the named instance only; a nested require
                // takes its own install first.
                if who == instance_id {
                    return Err(format!(
                        "need proxy slot for {who}; pass --slot <dxgi|d3d9|d3d10|d3d11|d3d12|winmm|version|<self>>"
                    )
                    .into());
                }
                return Err(format!(
                    "need proxy slot for {who}; install it first: tuxgt instance install {game} {who} --slot <dxgi|d3d9|d3d10|d3d11|d3d12|winmm|version|<self>>"
                )
                .into());
            }
            Err(Error::NeedConfirm(msg)) if !opts.yes => {
                confirm_list("foreign game-dir dests", &msg_items(&msg), false)?;
                opts.yes = true;
            }
            Err(Error::ArchivePasswordRequired) if password_from_flag => {
                return Err("archive password required".into());
            }
            Err(Error::ArchivePasswordRequired) => {
                opts.password = Some(read_archive_password()?);
            }
            Err(e) => return Err(e.into()),
        }
    }
}
