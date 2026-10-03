//! App release versions: `vX.Y.Z` with optional `-beta.N`.

/// Parsed app version. A final beats any beta of the same triple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// `None` = final release; `Some(n)` = `-beta.n`.
    pub beta: Option<u64>,
}

impl Ord for AppVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.beta, other.beta) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(a), Some(b)) => a.cmp(&b),
            })
    }
}

impl PartialOrd for AppVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Parse `v0.9.1`, `0.9.1`, `v0.10.0-beta.2`. Anything else is `None` —
/// callers report `Unknown`, never an error.
pub fn parse_app_version(s: &str) -> Option<AppVersion> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let (triple, beta) = match s.split_once("-beta.") {
        Some((t, b)) => (t, Some(b.parse::<u64>().ok()?)),
        None => {
            if s.contains('-') {
                return None;
            }
            (s, None)
        }
    };
    let mut it = triple.split('.');
    let major = it.next()?.parse::<u64>().ok()?;
    let minor = it.next()?.parse::<u64>().ok()?;
    let patch = it.next()?.parse::<u64>().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some(AppVersion {
        major,
        minor,
        patch,
        beta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags_and_plain_versions() {
        let v = parse_app_version("v0.9.1").expect("tag");
        assert_eq!(
            v,
            AppVersion {
                major: 0,
                minor: 9,
                patch: 1,
                beta: None,
            }
        );
        assert_eq!(v, parse_app_version("0.9.1").expect("plain"));
        assert_eq!(
            parse_app_version("v0.10.0-beta.2").expect("beta"),
            AppVersion {
                major: 0,
                minor: 10,
                patch: 0,
                beta: Some(2),
            }
        );
    }

    #[test]
    fn rejects_non_release_shapes() {
        for bad in [
            "",
            "v1",
            "v1.2",
            "v1.2.3.4",
            "v1.2.x",
            "release-1",
            "v1.2.3-rc.1",
            "v1.2.3-beta.x",
            "v1.2.3-beta.",
        ] {
            assert_eq!(parse_app_version(bad), None, "{bad}");
        }
    }

    #[test]
    fn orders_triples_then_beta_then_final() {
        let v = |s: &str| parse_app_version(s).expect(s);
        assert!(v("v0.9.0") < v("v0.9.1"));
        assert!(v("v0.9.1") < v("v0.10.0"));
        assert!(v("v0.10.0-beta.1") < v("v0.10.0-beta.2"));
        assert!(v("v0.10.0-beta.2") < v("v0.10.0"));
        assert!(v("v0.9.9") < v("v0.10.0-beta.1"));
        assert_eq!(v("v1.2.3"), v("1.2.3"));
    }

    #[test]
    fn workspace_version_parses() {
        // Guards Cargo.toml vs release-tag drift: the running version must
        // always be comparable.
        assert!(parse_app_version(env!("CARGO_PKG_VERSION")).is_some());
    }
}
