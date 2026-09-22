use super::*;
/// `wrapper list [<game-id>] | set <game-id> <wrapper> | unset <game-id> <wrapper>`.
/// Only defs of the enabled `wrapper` plugin are offered; a disabled plugin
/// resolves every wrapper as unknown (stored rows stay, E12 rule).
pub(crate) async fn run(
    pool: &SqlitePool,
    host: &PluginHost,
    dir: &std::path::Path,
    cmd: WrapperCmd,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        WrapperCmd::List { id } => match id {
            None => {
                for d in wrapper_defs(host) {
                    println!("{}\t{}", d.id, d.help);
                }
            }
            Some(id) => {
                if !game_exists(pool, &id).await? {
                    return Err(Error::UnknownGame(id).into());
                }
                let on = game_wrappers(pool, &id).await?;
                for d in wrapper_defs(host) {
                    let state = if on.iter().any(|w| w == d.id) {
                        "on"
                    } else {
                        "off"
                    };
                    println!("{}\t{}\t{}", d.id, state, d.help);
                }
            }
        },
        WrapperCmd::Set { id, wrapper } => {
            find_enabled_wrapper(host, &wrapper)
                .ok_or_else(|| Error::UnknownWrapper(wrapper.clone()))?;
            mutate_game(pool, dir, host, &id, set_wrapper(pool, &id, &wrapper)).await?;
            tracing::info!(game = id.as_str(), wrapper = wrapper.as_str(), "wrapper set");
        }
        WrapperCmd::Unset { id, wrapper } => {
            find_enabled_wrapper(host, &wrapper)
                .ok_or_else(|| Error::UnknownWrapper(wrapper.clone()))?;
            mutate_game(pool, dir, host, &id, unset_wrapper(pool, &id, &wrapper)).await?;
            tracing::info!(game = id.as_str(), wrapper = wrapper.as_str(), "wrapper unset");
        }
    }
    Ok(())
}
