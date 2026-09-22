use tuxgt_core::{FluentArgs, GameRow, Strings};

use super::super::super::widgets;

/// Platform filter value: `{family}` or `{family}:{api}` (store-key style).
/// Family is `windows` (`proton`/`wine`) or `native`; api is the stored id
/// (`dx9`..`vulkan`). `None` = All.
pub(crate) fn parse_platform_key(key: &str) -> (&str, Option<&str>) {
    match key.split_once(':') {
        Some((f, a)) => (f, Some(a)),
        None => (key, None),
    }
}

/// Runner family of one game. Unset or unknown platforms belong to no family.
pub(crate) fn platform_family(platform: Option<&str>) -> Option<&'static str> {
    match platform {
        Some("native") => Some("native"),
        Some("proton") | Some("wine") => Some("windows"),
        _ => None,
    }
}

/// Combined runner+API match. Unset platform matches no family arm; unset API
/// matches the family row but no API row. Unknown families never match.
pub(crate) fn platform_matches(
    filter: Option<&str>,
    platform: Option<&str>,
    api: Option<&str>,
) -> bool {
    let Some(key) = filter else {
        return true;
    };
    let (family, want_api) = parse_platform_key(key);
    if platform_family(platform) != Some(family) {
        return false;
    }
    match want_api {
        None => true,
        Some(a) => api == Some(a),
    }
}

/// Display label for one runner family. Native games are Linux games, so the
/// dropdown says Linux, not the stored `native` id.
fn family_label(family: &str, strings: &Strings) -> String {
    match family {
        "native" => strings.get("gui-platform-linux"),
        _ => strings.get("gui-filter-windows"),
    }
}

/// `{family} - {api|All}` label through the shared join template.
fn platform_label(family: &str, second: String, strings: &Strings) -> String {
    let mut args = FluentArgs::new();
    args.set("platform", family_label(family, strings));
    args.set("api", second);
    strings.get_args("gui-filter-platform-api", Some(&args))
}

/// Platform options: both family rows always (Windows first), then one row
/// per observed (family, api) pair, APIs sorted. Values stay ids.
pub(crate) fn platform_options(games: &[GameRow], strings: &Strings) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for family in ["windows", "native"] {
        out.push((
            family.to_string(),
            platform_label(family, strings.get("gui-filter-all"), strings),
        ));
        let mut apis: Vec<&str> = Vec::new();
        for g in games {
            if platform_family(g.platform.as_deref()) != Some(family) {
                continue;
            }
            if let Some(a) = g.api.as_deref() {
                if !a.is_empty() && !apis.contains(&a) {
                    apis.push(a);
                }
            }
        }
        apis.sort_unstable();
        for api in apis {
            out.push((
                format!("{family}:{api}"),
                platform_label(
                    family,
                    widgets::id_label(widgets::ValKind::Api, api, strings),
                    strings,
                ),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{platform_matches, platform_options};
    use tuxgt_core::{GameRow, Strings};

    fn row(id: &str, platform: Option<&str>, api: Option<&str>) -> GameRow {
        GameRow {
            id: id.into(),
            name: Some(id.into()),
            cover_path: None,
            manager: "steam".into(),
            store: String::new(),
            header_path: None,
            platform: platform.map(str::to_string),
            api: api.map(str::to_string),
            install_dir: None,
            exe_path: None,
            prefix_path: None,
            proton: None,
            bitness: None,
            engine: None,
            hidden: false,
            last_played: None,
            steam_appid: None,
        }
    }

    #[test]
    fn platform_matches_all_and_unset() {
        assert!(platform_matches(None, None, None));
        assert!(platform_matches(None, Some("native"), Some("vulkan")));
        assert!(!platform_matches(Some("native"), None, None));
        assert!(!platform_matches(Some("windows"), None, None));
    }

    #[test]
    fn platform_matches_native_vs_windows() {
        assert!(platform_matches(Some("native"), Some("native"), None));
        assert!(!platform_matches(Some("native"), Some("proton"), None));
        assert!(platform_matches(Some("windows"), Some("proton"), None));
        assert!(platform_matches(Some("windows"), Some("wine"), None));
        assert!(!platform_matches(Some("windows"), Some("native"), None));
    }

    #[test]
    fn platform_matches_api_arm() {
        assert!(platform_matches(
            Some("windows:dx12"),
            Some("proton"),
            Some("dx12")
        ));
        assert!(!platform_matches(
            Some("windows:dx12"),
            Some("proton"),
            Some("vulkan")
        ));
        assert!(!platform_matches(
            Some("windows:dx12"),
            Some("proton"),
            None
        ));
        assert!(!platform_matches(
            Some("windows:dx12"),
            Some("native"),
            Some("dx12")
        ));
        assert!(platform_matches(
            Some("native:vulkan"),
            Some("native"),
            Some("vulkan")
        ));
    }

    #[test]
    fn platform_matches_unknown_family_never() {
        assert!(!platform_matches(Some("proton"), Some("proton"), None));
        assert!(!platform_matches(Some("dos"), Some("wine"), Some("vga")));
    }

    #[test]
    fn platform_options_family_rows_always_first() {
        let s = Strings::en_us().expect("catalog");
        let opts = platform_options(&[], &s);
        let vals: Vec<&str> = opts.iter().map(|(v, _)| v.as_str()).collect();
        assert_eq!(vals, vec!["windows", "native"]);
        let map: std::collections::HashMap<_, _> = opts.into_iter().collect();
        assert_eq!(
            map.get("windows").map(String::as_str),
            Some("Windows - All")
        );
        assert_eq!(map.get("native").map(String::as_str), Some("Linux - All"));
    }

    #[test]
    fn platform_options_lists_observed_pairs_sorted() {
        let s = Strings::en_us().expect("catalog");
        let games = vec![
            row("a", Some("proton"), Some("vulkan")),
            row("b", Some("wine"), Some("dx12")),
            row("c", Some("native"), Some("vulkan")),
            row("d", None, Some("dx12")),
            row("e", Some("proton"), None),
        ];
        let opts = platform_options(&games, &s);
        let vals: Vec<&str> = opts.iter().map(|(v, _)| v.as_str()).collect();
        assert_eq!(
            vals,
            vec![
                "windows",
                "windows:dx12",
                "windows:vulkan",
                "native",
                "native:vulkan"
            ]
        );
        let map: std::collections::HashMap<_, _> = opts.into_iter().collect();
        assert_eq!(
            map.get("windows:dx12").map(String::as_str),
            Some("Windows - DX12")
        );
        assert_eq!(
            map.get("native:vulkan").map(String::as_str),
            Some("Linux - Vulkan")
        );
    }
}
