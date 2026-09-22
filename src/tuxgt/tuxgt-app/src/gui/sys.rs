use std::fs;

use tuxgt_core::{FluentArgs, Strings};

#[derive(Clone, Debug)]
pub struct SysInfo {
    pub os: String,
    pub desktop: String,
    pub gpu: String,
    pub cpu: String,
    pub ram: String,
}

impl SysInfo {
    pub fn gather(strings: &Strings) -> Self {
        let unknown = strings.get("gui-sys-unknown");
        let outs = kk_outputs();
        Self {
            os: os_kernel(strings),
            desktop: desktop_from(&outs, &unknown),
            gpu: gpu_from(&outs, &unknown),
            cpu: cpu_name(strings),
            ram: ram_label(strings),
        }
    }
}

/// One-shot kkfetch collection for the rows kkfetch owns (desktop, GPU).
/// `Config::default()` never reads the user's kkfetch config file, and
/// `no_plugins` never spawns user executables. All collectors run; each
/// row filters the outputs it owns.
fn kk_outputs() -> Vec<kkfetch::modules::ModuleOutput> {
    let cli = kkfetch::cli::Cli {
        no_plugins: true,
        ..Default::default()
    };
    let ctx =
        kkfetch::context::FetchContext::with_config(&cli, kkfetch::config::Config::default());
    kkfetch::modules::ModuleRegistry::new().collect_all(&ctx)
}

/// Desktop row from kkfetch. Its `Desktop` value already folds DE, WM, and
/// session (`... (Wayland)`, `... (WM: W, ...)`), so it is used verbatim
/// when the WM adds nothing (see below); a genuinely distinct WM is
/// appended, and the separate `Wm` value backs bare-WM sessions.
fn desktop_from(outs: &[kkfetch::modules::ModuleOutput], unknown: &str) -> String {
    use kkfetch::modules::ModuleId;
    let first = |id| {
        outs.iter()
            .filter(|o| o.id == id)
            .map(|o| o.value.trim())
            .find(|v| !v.is_empty())
    };
    match (first(ModuleId::Desktop), first(ModuleId::Wm)) {
        (None, None) => unknown.to_string(),
        (None, Some(w)) => w.to_string(),
        (Some(d), None) => d.to_string(),
        (Some(d), Some(w)) if wm_covered_by(d, w) => d.to_string(),
        (Some(d), Some(w)) => format!("{d} · {w}"),
    }
}

/// Whether the WM adds nothing to the Desktop value: named inline, or the
/// DE's default (mirrors kkfetch `format_desktop_info` fold arms, which
/// elide exactly these WMs from the Desktop value).
fn wm_covered_by(desktop: &str, wm: &str) -> bool {
    let d = desktop.to_lowercase();
    let w = wm.to_lowercase();
    if d.contains(&w) {
        return true;
    }
    [
        ("gnome", "mutter"),
        ("kde", "kwin"),
        ("xfce", "xfwm"),
        ("cinnamon", "muffin"),
        ("mate", "marco"),
        ("mate", "metacity"),
        ("lxqt", "kwin"),
        ("lxqt", "openbox"),
    ]
    .iter()
    .any(|(de, default_wm)| d.contains(de) && w.contains(default_wm))
}

/// GPU row from kkfetch: one line per GPU in collector order. The upstream
/// VRAM estimate is dropped (wrong on some cards, no signal for us); the
/// name plus `[Discrete]`/`[Integrated]` tag stays verbatim. Empty maps to
/// unknown.
fn gpu_from(outs: &[kkfetch::modules::ModuleOutput], unknown: &str) -> String {
    let names: Vec<String> = outs
        .iter()
        .filter(|o| o.id == kkfetch::modules::ModuleId::Gpu)
        .map(|o| strip_vram(o.value.trim()))
        .filter(|v| !v.is_empty())
        .collect();
    if names.is_empty() {
        unknown.to_string()
    } else {
        names.join("\n")
    }
}

/// Drop the upstream ` (32 GiB)` VRAM group; everything else verbatim.
fn strip_vram(value: &str) -> String {
    if let Some(open) = value.find(" (") {
        if let Some(rel) = value[open..].find(')') {
            let inner = &value[open + 2..open + rel];
            if inner.ends_with("GiB") || inner.ends_with("MiB") {
                return format!("{}{}", &value[..open], &value[open + rel + 1..]);
            }
        }
    }
    value.to_string()
}

fn os_pretty() -> String {
    if let Ok(text) = fs::read_to_string("/etc/os-release") {
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                return v.trim_matches('"').to_string();
            }
        }
    }
    "Linux".into()
}

fn kernel_release(strings: &Strings) -> String {
    fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| strings.get("gui-sys-unknown"))
}

fn os_kernel(strings: &Strings) -> String {
    let mut kargs = FluentArgs::new();
    kargs.set("release", kernel_release(strings));
    let kernel = strings.get_args("gui-sys-kernel", Some(&kargs));
    let mut args = FluentArgs::new();
    args.set("os", os_pretty());
    args.set("kernel", kernel);
    strings.get_args("gui-sys-os-kernel", Some(&args))
}

fn cpu_name(strings: &Strings) -> String {
    let Ok(text) = fs::read_to_string("/proc/cpuinfo") else {
        return strings.get("gui-sys-unknown");
    };
    let mut hardware = None;
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        if v.is_empty() {
            continue;
        }
        match k.trim() {
            "model name" => return strip_cpu_suffix(v).to_string(),
            "Hardware" if hardware.is_none() => hardware = Some(v.to_string()),
            _ => {}
        }
    }
    hardware
        .map(|s| strip_cpu_suffix(&s).to_string())
        .unwrap_or_else(|| strings.get("gui-sys-unknown"))
}

fn strip_cpu_suffix(name: &str) -> &str {
    let mut s = name.trim();
    loop {
        let next = s
            .strip_suffix(" CPU")
            .or_else(|| s.strip_suffix(" Processor"))
            .or_else(|| strip_trailing_n_core(s))
            .unwrap_or(s)
            .trim_end();
        if next.len() == s.len() {
            return s;
        }
        s = next;
    }
}

fn strip_trailing_n_core(s: &str) -> Option<&str> {
    let (head, last) = s.rsplit_once(' ')?;
    let last = last
        .strip_suffix(['s', 'S'])
        .filter(|rest| rest.len() < last.len())
        .unwrap_or(last);
    let (n, core) = last.rsplit_once('-')?;
    if core.eq_ignore_ascii_case("core") && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
        Some(head)
    } else {
        None
    }
}

fn ram_label(strings: &Strings) -> String {
    let Some(kb) = mem_total_kb() else {
        return strings.get("gui-sys-unknown");
    };
    let mut args = FluentArgs::new();
    args.set("gib", (kb / 1_048_576).to_string());
    strings.get_args("gui-sys-ram", Some(&args))
}

fn mem_total_kb() -> Option<u64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("MemTotal:") else {
            continue;
        };
        return rest.split_whitespace().next()?.parse().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{desktop_from, gpu_from, strip_cpu_suffix, strip_trailing_n_core};
    use kkfetch::modules::{ModuleId, ModuleOutput};

    fn out(id: ModuleId, value: &str) -> ModuleOutput {
        ModuleOutput {
            id,
            label: String::new(),
            value: value.to_string(),
            custom_rendered: None,
        }
    }

    #[test]
    fn cpu_trailing_suffix() {
        assert_eq!(
            strip_cpu_suffix("Intel(R) Core(TM) i7-8700 CPU"),
            "Intel(R) Core(TM) i7-8700"
        );
        assert_eq!(strip_cpu_suffix("ARMv8 Processor"), "ARMv8");
        assert_eq!(
            strip_cpu_suffix("AMD Ryzen 7 5800X3D"),
            "AMD Ryzen 7 5800X3D"
        );
        assert_eq!(
            strip_cpu_suffix("AMD Ryzen 5 3600 6-Core Processor"),
            "AMD Ryzen 5 3600"
        );
        assert_eq!(
            strip_trailing_n_core("AMD Ryzen 5 3600 6-Core"),
            Some("AMD Ryzen 5 3600")
        );
    }

    #[test]
    fn desktop_implied_wm_stays_verbatim() {
        // kkfetch folds the default WM into the Desktop value.
        assert_eq!(
            desktop_from(
                &[
                    out(ModuleId::Desktop, "GNOME 50.5 (Wayland)"),
                    out(ModuleId::Wm, "Mutter"),
                ],
                "unknown",
            ),
            "GNOME 50.5 (Wayland)"
        );
        assert_eq!(
            desktop_from(
                &[
                    out(ModuleId::Desktop, "KDE Plasma (Wayland)"),
                    out(ModuleId::Wm, "KWin"),
                ],
                "unknown",
            ),
            "KDE Plasma (Wayland)"
        );
    }

    #[test]
    fn desktop_appends_distinct_wm() {
        assert_eq!(
            desktop_from(
                &[out(ModuleId::Desktop, "Plasma (Wayland)"), out(ModuleId::Wm, "KWin")],
                "unknown",
            ),
            "Plasma (Wayland) · KWin"
        );
        assert_eq!(
            desktop_from(&[out(ModuleId::Wm, "Sway")], "unknown"),
            "Sway"
        );
        assert_eq!(desktop_from(&[], "unknown"), "unknown");
    }

    #[test]
    fn gpu_strips_vram_and_stacks_lines() {
        assert_eq!(
            gpu_from(
                &[
                    out(ModuleId::Gpu, "TestVendor TV-9000 (8 GiB) [Discrete]"),
                    out(ModuleId::Gpu, "Example EG-100 (512 MiB) [Integrated]"),
                ],
                "unknown",
            ),
            "TestVendor TV-9000 [Discrete]\nExample EG-100 [Integrated]"
        );
        assert_eq!(
            gpu_from(&[out(ModuleId::Gpu, "PCI Display (1234:5678)")], "unknown"),
            "PCI Display (1234:5678)"
        );
        assert_eq!(gpu_from(&[], "unknown"), "unknown");
    }

    #[test]
    fn ram_floor_gib() {
        assert_eq!(32, 33_554_432u64 / 1_048_576);
        assert_eq!(31, 33_554_431u64 / 1_048_576);
        assert_eq!(0, 1_048_575u64 / 1_048_576);
    }
}
