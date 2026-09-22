//! Test-only fixture builders (T07: one `games` seeder, no SQL in tests).
use sqlx::SqlitePool;

/// Row seed for the `games` table. Tests pass a struct, not SQL strings.
///
/// `manager`/`store`/`game_id` default to `""` (matching the schema's
/// `NOT NULL DEFAULT ''`); every other column defaults to NULL, i.e. the
/// same row as an `INSERT` that omits the column.
#[derive(Default)]
pub(crate) struct SeedGame<'a> {
    pub id: &'a str,
    pub manager: &'a str,
    pub store: &'a str,
    pub game_id: &'a str,
    pub name: Option<&'a str>,
    pub install_dir: Option<&'a str>,
    pub exe_path: Option<&'a str>,
    pub prefix_path: Option<&'a str>,
    pub proton: Option<&'a str>,
    pub launch_options: Option<&'a str>,
    pub env: Option<&'a str>,
    pub wrapper: Option<&'a str>,
    pub detected_exe_path: Option<&'a str>,
    pub detected_platform: Option<&'a str>,
    pub detected_api: Option<&'a str>,
    pub detected_bitness: Option<&'a str>,
    pub detected_proton: Option<&'a str>,
    pub override_exe_path: Option<&'a str>,
    pub override_api: Option<&'a str>,
    pub override_platform: Option<&'a str>,
    pub detected_hidden: Option<i64>,
    pub override_hidden: Option<i64>,
}

pub(crate) async fn seed_game(pool: &SqlitePool, g: SeedGame<'_>) {
    sqlx::query(
        "INSERT INTO games (id, manager, store, game_id, name, install_dir, exe_path, prefix_path,
         proton, launch_options, env, wrapper, detected_exe_path, detected_platform, detected_api,
         detected_bitness, detected_proton, override_exe_path, override_api, override_platform,
         detected_hidden, override_hidden)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(g.id)
    .bind(g.manager)
    .bind(g.store)
    .bind(g.game_id)
    .bind(g.name)
    .bind(g.install_dir)
    .bind(g.exe_path)
    .bind(g.prefix_path)
    .bind(g.proton)
    .bind(g.launch_options)
    .bind(g.env)
    .bind(g.wrapper)
    .bind(g.detected_exe_path)
    .bind(g.detected_platform)
    .bind(g.detected_api)
    .bind(g.detected_bitness)
    .bind(g.detected_proton)
    .bind(g.override_exe_path)
    .bind(g.override_api)
    .bind(g.override_platform)
    .bind(g.detected_hidden)
    .bind(g.override_hidden)
    .execute(pool)
    .await
    .expect("seed_game insert");
}
