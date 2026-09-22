use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq, sqlx::FromRow)]
pub struct GameRow {
    pub id: String,
    pub name: Option<String>,
    pub cover_path: Option<String>,
    pub manager: String,
    pub store: String,
    pub header_path: Option<String>,
    /// `override_platform` if set, else `detected_platform`.
    pub platform: Option<String>,
    /// `override_api` if set, else `detected_api`.
    pub api: Option<String>,
    pub install_dir: Option<String>,
    pub exe_path: Option<String>,
    pub prefix_path: Option<String>,
    pub proton: Option<String>,
    pub bitness: Option<String>,
    pub engine: Option<String>,
    /// Effective hidden: `override_hidden` wins, else `detected_hidden`, else visible.
    /// Display-layer only; `list_games` keeps returning all rows.
    pub hidden: bool,
    /// R01: last-played unix seconds, NULL = never played. Set by
    /// `touch_last_played` on Play; scan upsert never writes it.
    pub last_played: Option<i64>,
    /// games.steam_appid user overlay (E43), raw. None when unset; use resolved_appid() for the metadata appid.
    pub steam_appid: Option<String>,
}

impl GameRow {
    pub fn local_art(&self) -> Option<PathBuf> {
        for p in [&self.cover_path, &self.header_path] {
            if let Some(s) = p {
                let path = PathBuf::from(s);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Remote cover source a scan recorded (Heroic library art is remote
    /// URLs). The GUI fetches it lazily; `local_art` still needs a file.
    pub fn art_url(&self) -> Option<&str> {
        [&self.cover_path, &self.header_path]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .find(|s| s.starts_with("http://") || s.starts_with("https://"))
    }

    pub fn display_name(&self) -> &str {
        display_name_of(&self.id, self.name.as_deref())
    }

    /// E43: metadata AppID — stored overlay wins, else the Steam game segment
    /// when `manager == "steam"`, else `None` (`docs/dev/app/core/identity.md`).
    /// An empty stored value counts as unset, like `steam_appid_of`.
    pub fn resolved_appid(&self) -> Option<&str> {
        self.steam_appid
            .as_deref()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                (self.manager == "steam")
                    .then(|| self.id.split(':').nth(2).filter(|s| !s.is_empty()))
                    .flatten()
            })
    }

    pub fn initials(&self) -> String {
        initials_of(self.display_name())
    }
}

/// Display name shared by full and index rows: the name, unless missing or
/// blank, else the id.
fn display_name_of<'a>(id: &'a str, name: Option<&'a str>) -> &'a str {
    name.filter(|s| !s.is_empty()).unwrap_or(id)
}

/// Initials shared by full and index rows: first alphanumerics of the first
/// two words, uppercased, else `?`.
fn initials_of(display: &str) -> String {
    let mut out = String::new();
    for w in display.split(|c: char| !c.is_alphanumeric()) {
        if let Some(c) = w.chars().next() {
            out.push(c.to_ascii_uppercase());
            if out.len() >= 2 {
                break;
            }
        }
    }
    if out.is_empty() {
        out.push('?');
    }
    out
}

/// Always-held library entry: the sidebar paints name, hidden state, and
/// id-keyed art from this alone. Full rows load per page (Library: all,
/// Game: selected).
#[derive(Clone, Debug, Eq, PartialEq, sqlx::FromRow)]
pub struct GameIndexRow {
    pub id: String,
    pub name: Option<String>,
    pub manager: String,
    /// Effective hidden: `override_hidden` wins, else `detected_hidden`.
    pub hidden: bool,
}

impl GameIndexRow {
    /// Minimal entry from a full row (scan/rescan fold-in).
    pub fn from_row(g: &GameRow) -> Self {
        Self {
            id: g.id.clone(),
            name: g.name.clone(),
            manager: g.manager.clone(),
            hidden: g.hidden,
        }
    }

    pub fn display_name(&self) -> &str {
        display_name_of(&self.id, self.name.as_deref())
    }

    pub fn initials(&self) -> String {
        initials_of(self.display_name())
    }
}
