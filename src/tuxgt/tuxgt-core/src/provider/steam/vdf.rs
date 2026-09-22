use crate::{Error, Result};

/// Compose our wrapper into existing launch options (compose, don't
/// clobber): keep the user's options, run them through our launcher,
/// then `%command%`.
pub(crate) fn compose_options(current: Option<&str>, frag: &str) -> String {
    match current.map(str::trim).filter(|s| !s.is_empty()) {
        None => format!("{frag} %command%"),
        Some(c) if c.contains("%command%") => {
            c.replacen("%command%", &format!("{frag} %command%"), 1)
        }
        Some(c) => format!("{c} {frag} %command%"),
    }
}

/// Strip our fragment back out (record-lost re-apply reconstruction).
pub(crate) fn strip_fragment(options: &str, frag: &str) -> Option<String> {
    let mut s = options.replacen(frag, "", 1);
    s = s.replace("%command%", "");
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

pub(crate) fn leading_ws(line: &str) -> &str {
    let i = line.len() - line.trim_start().len();
    &line[..i]
}

/// Track VDF brace depth outside quoted strings. Returns the depth delta
/// of the line and whether the line ends inside a string.
pub(crate) fn scan_line(line: &str, mut in_string: bool) -> (i32, bool) {
    let mut delta = 0;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '\\' {
                chars.next();
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if c == '{' {
            delta += 1;
        } else if c == '}' {
            delta -= 1;
        }
    }
    (delta, in_string)
}

pub(crate) fn vdf_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn vdf_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse the `LaunchOptions` value of one app block. `block_open` is the
/// line index of the app's `{`, `block_close` of its matching `}`.
pub(crate) fn block_options(
    lines: &[&str],
    block_open: usize,
    block_close: usize,
) -> Option<String> {
    for line in &lines[block_open + 1..block_close] {
        let t = line.trim();
        if t.starts_with("\"LaunchOptions\"") {
            let rest = &t["\"LaunchOptions\"".len()..];
            if let Some(v) = first_quoted(rest) {
                return Some(v);
            }
        }
    }
    None
}

/// First quoted string on the line (VDF value), unescaped.
pub(crate) fn first_quoted(line: &str) -> Option<String> {
    let mut chars = line.chars();
    loop {
        match chars.next()? {
            '"' => break,
            _ => {}
        }
    }
    let mut raw = String::new();
    loop {
        match chars.next()? {
            '\\' => {
                raw.push('\\');
                raw.push(chars.next()?);
            }
            '"' => return Some(vdf_unescape(&raw)),
            c => raw.push(c),
        }
    }
}

/// Byte-preserving surgical edit of `localconfig.vdf`: set the app's
/// `LaunchOptions`, creating the app block when missing. Returns the new
/// text and the previous value (None = the key was absent).
pub(crate) fn steam_set_options(
    text: &str,
    app: &str,
    options: &str,
) -> Result<(String, Option<String>)> {
    let owned = text.to_string();
    let lines: Vec<&str> = owned.split('\n').collect();
    let apps_idx = lines
        .iter()
        .position(|l| l.trim() == "\"apps\"")
        .ok_or_else(|| Error::Apply("localconfig.vdf has no apps section".into()))?;
    let open_idx = lines[apps_idx + 1..]
        .iter()
        .position(|l| l.trim() == "{")
        .map(|i| apps_idx + 1 + i)
        .ok_or_else(|| Error::Apply("localconfig.vdf apps section is malformed".into()))?;
    let key = format!("\"{app}\"");
    // Walk the apps object: depth relative to its opening brace.
    let mut depth = 0;
    let mut in_string = false;
    let mut app_open: Option<usize> = None;
    let mut app_close: Option<usize> = None;
    let mut apps_close: Option<usize> = None;
    let mut child_indent: Option<String> = None;
    let mut i = open_idx;
    while i < lines.len() {
        let t = lines[i].trim();
        if depth == 1 && t.starts_with('"') && first_quoted(t).is_some() {
            if child_indent.is_none() {
                child_indent = Some(leading_ws(lines[i]).to_string());
            }
            if t == key {
                // The app block opens on a following line.
                let mut j = i + 1;
                while j < lines.len() && lines[j].trim().is_empty() {
                    j += 1;
                }
                if j < lines.len() && lines[j].trim() == "{" {
                    let mut d = 1;
                    let mut s = false;
                    let mut k = j + 1;
                    while k < lines.len() {
                        let (dd, ss) = scan_line(lines[k], s);
                        s = ss;
                        d += dd;
                        if d == 0 {
                            break;
                        }
                        k += 1;
                    }
                    if d != 0 {
                        return Err(Error::Apply(format!(
                            "localconfig.vdf app {app} block is malformed"
                        )));
                    }
                    app_open = Some(j);
                    app_close = Some(k);
                    break;
                }
            }
        }
        let (dd, ss) = scan_line(lines[i], in_string);
        in_string = ss;
        depth += dd;
        if depth < 0 {
            return Err(Error::Apply(
                "localconfig.vdf apps section is malformed".into(),
            ));
        }
        if depth == 0 {
            apps_close = Some(i);
            break;
        }
        i += 1;
    }
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    if let (Some(open), Some(close)) = (app_open, app_close) {
        let previous = block_options(&lines, open, close);
        let indent = leading_ws(lines[open]).to_string() + "\t";
        let newline = format!("{indent}\"LaunchOptions\"\t\t\"{}\"", vdf_escape(options));
        let mut replaced = false;
        for line in &mut out[open + 1..close] {
            if line.trim().starts_with("\"LaunchOptions\"") {
                *line = newline.clone();
                replaced = true;
                break;
            }
        }
        if !replaced {
            out.insert(open + 1, newline);
        }
        Ok((out.join("\n"), previous))
    } else {
        let close = apps_close
            .ok_or_else(|| Error::Apply("localconfig.vdf apps section never closes".into()))?;
        let apps_indent = leading_ws(lines[apps_idx]).to_string();
        let ci = child_indent.unwrap_or_else(|| apps_indent.clone() + "\t");
        let esc = vdf_escape(options);
        out.insert(
            close,
            format!("{ci}{key}\n{ci}{{\n{ci}\t\"LaunchOptions\"\t\t\"{esc}\"\n{ci}}}"),
        );
        Ok((out.join("\n"), None))
    }
}

/// Read one app's `LaunchOptions` without modifying the text.
pub(crate) fn steam_get_options(text: &str, app: &str) -> Option<String> {
    let owned = text.to_string();
    let lines: Vec<&str> = owned.split('\n').collect();
    let apps_idx = lines.iter().position(|l| l.trim() == "\"apps\"")?;
    let open_idx = lines[apps_idx + 1..]
        .iter()
        .position(|l| l.trim() == "{")
        .map(|i| apps_idx + 1 + i)?;
    let key = format!("\"{app}\"");
    let mut depth = 0;
    let mut in_string = false;
    let mut i = open_idx;
    while i < lines.len() {
        let t = lines[i].trim();
        if depth == 1 && t == key {
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < lines.len() && lines[j].trim() == "{" {
                let mut d = 1;
                let mut s = false;
                let mut k = j + 1;
                while k < lines.len() {
                    let (dd, ss) = scan_line(lines[k], s);
                    s = ss;
                    d += dd;
                    if d == 0 {
                        break;
                    }
                    k += 1;
                }
                if d == 0 {
                    return block_options(&lines, j, k);
                }
                return None;
            }
        }
        let (dd, ss) = scan_line(lines[i], in_string);
        in_string = ss;
        depth += dd;
        if depth < 0 {
            break;
        }
        i += 1;
    }
    None
}

/// Live `LaunchOptions` for a Steam app, read from every local user's
/// `localconfig.vdf` (last non-empty value wins). `None` when Steam holds
/// nothing for the app. Sync file read for the About read-only reference;
/// the launch path never consults it.
pub fn steam_launch_options(app: &str) -> Option<String> {
    let dirs = steamlocate::locate_all().ok()?;
    let mut out: Option<String> = None;
    for dir in dirs {
        let userdata = dir.path().join("userdata");
        let Ok(users) = std::fs::read_dir(&userdata) else {
            continue;
        };
        for user in users.flatten() {
            let path = user.path().join("config").join("localconfig.vdf");
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(opt) = steam_get_options(&text, app)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                out = Some(opt);
            }
        }
    }
    out
}

/// Remove one app's `LaunchOptions` line (key-absent restore). Leaves an
/// emptied block in place; Steam treats it like any untouched entry.
pub(crate) fn steam_remove_options(text: &str, app: &str) -> String {
    let owned = text.to_string();
    let lines: Vec<&str> = owned.split('\n').collect();
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    let apps_idx = match lines.iter().position(|l| l.trim() == "\"apps\"") {
        Some(i) => i,
        None => return owned,
    };
    let open_idx = match lines[apps_idx + 1..]
        .iter()
        .position(|l| l.trim() == "{")
        .map(|i| apps_idx + 1 + i)
    {
        Some(i) => i,
        None => return owned,
    };
    let key = format!("\"{app}\"");
    let mut depth = 0;
    let mut in_string = false;
    let mut i = open_idx;
    while i < out.len() {
        let is_key = {
            let t = out[i].trim();
            depth == 1 && t == key
        };
        if is_key {
            let mut j = i + 1;
            while j < out.len() && out[j].trim().is_empty() {
                j += 1;
            }
            if j < out.len() && out[j].trim() == "{" {
                let mut d = 1;
                let mut s = false;
                let mut k = j + 1;
                while k < out.len() {
                    let (dd, ss) = scan_line(&out[k], s);
                    s = ss;
                    d += dd;
                    if d == 0 {
                        break;
                    }
                    k += 1;
                }
                if d == 0 {
                    out.drain(
                        (j + 1..k)
                            .find(|&m| out[m].trim().starts_with("\"LaunchOptions\""))
                            .map(|m| m..m + 1)
                            .unwrap_or(k..k),
                    );
                }
                break;
            }
        }
        let (dd, ss) = scan_line(&out[i], in_string);
        in_string = ss;
        depth += dd;
        if depth < 0 {
            break;
        }
        i += 1;
    }
    out.join("\n")
}
