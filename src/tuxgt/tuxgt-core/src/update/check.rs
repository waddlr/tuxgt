//! Latest-release check for the app itself: own version against the
//! newest non-prerelease `waddlr/tuxgt` release carrying `tuxgt.tar.gz`.

use super::{parse_app_version, UPDATE_ASSET, UPDATE_OWNER, UPDATE_REPO};
use crate::download::{github_release, GhAsset};

/// Running app version (`CARGO_PKG_VERSION`).
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Own-version vs latest-release verdict. A dead network, an unparseable
/// tag, or a release without the asset is `Unknown`, never an error that
/// blocks the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppUpdateStatus {
    UpToDate {
        current: String,
        tag: String,
    },
    Available {
        current: String,
        tag: String,
        asset_url: String,
    },
    Unknown {
        reason: String,
    },
}

/// Infallible by design: every failure mode is an `Unknown` verdict, so
/// callers have no error branch.
pub async fn check_app_update() -> AppUpdateStatus {
    let current = app_version();
    if let Some((tag, url)) = test_feed() {
        return compare(current, &tag, url);
    }
    let rel = match github_release(UPDATE_OWNER, UPDATE_REPO, None, false).await {
        Ok(rel) => rel,
        Err(e) => {
            return AppUpdateStatus::Unknown {
                reason: format!("cannot reach releases: {e}"),
            }
        }
    };
    let Some(asset_url) = select_app_asset(&rel.assets) else {
        return AppUpdateStatus::Unknown {
            reason: format!("{} ships no {UPDATE_ASSET}", rel.tag_name),
        };
    };
    compare(current, &rel.tag_name, asset_url)
}

/// Exact-name asset pick. Split out so the miss path is unit-testable
/// without network.
fn select_app_asset(assets: &[GhAsset]) -> Option<String> {
    assets
        .iter()
        .find(|a| a.name == UPDATE_ASSET)
        .map(|a| a.browser_download_url.clone())
}

fn compare(current: &str, tag: &str, asset_url: String) -> AppUpdateStatus {
    match (parse_app_version(current), parse_app_version(tag)) {
        (Some(have), Some(latest)) if latest > have => AppUpdateStatus::Available {
            current: current.into(),
            tag: tag.into(),
            asset_url,
        },
        (Some(_), Some(_)) => AppUpdateStatus::UpToDate {
            current: current.into(),
            tag: tag.into(),
        },
        _ => AppUpdateStatus::Unknown {
            reason: format!("cannot compare {current} with {tag}"),
        },
    }
}

/// Test seam (no production use): `TUXGT_TEST_UPDATE_TAG` +
/// `TUXGT_TEST_UPDATE_ASSET_URL` answer the check without network. The
/// `TEST` infix keeps it out of real environments.
fn test_feed() -> Option<(String, String)> {
    let tag = std::env::var("TUXGT_TEST_UPDATE_TAG")
        .ok()
        .filter(|s| !s.is_empty())?;
    let url = std::env::var("TUXGT_TEST_UPDATE_ASSET_URL")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some((tag, url))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env restore on drop: one test owns the feed vars, sequentially.
    struct FeedGuard {
        tag: Option<std::ffi::OsString>,
        url: Option<std::ffi::OsString>,
    }

    impl FeedGuard {
        fn set(tag: &str, url: &str) -> Self {
            let guard = Self {
                tag: std::env::var_os("TUXGT_TEST_UPDATE_TAG"),
                url: std::env::var_os("TUXGT_TEST_UPDATE_ASSET_URL"),
            };
            std::env::set_var("TUXGT_TEST_UPDATE_TAG", tag);
            std::env::set_var("TUXGT_TEST_UPDATE_ASSET_URL", url);
            guard
        }
    }

    impl Drop for FeedGuard {
        fn drop(&mut self) {
            match self.tag.take() {
                Some(v) => std::env::set_var("TUXGT_TEST_UPDATE_TAG", v),
                None => std::env::remove_var("TUXGT_TEST_UPDATE_TAG"),
            }
            match self.url.take() {
                Some(v) => std::env::set_var("TUXGT_TEST_UPDATE_ASSET_URL", v),
                None => std::env::remove_var("TUXGT_TEST_UPDATE_ASSET_URL"),
            }
        }
    }

    #[tokio::test]
    async fn check_compares_without_network() {
        // Unroutable URL: any network touch fails, so Available proves the
        // seam answered.
        let _feed = FeedGuard::set("v999.0.0", "http://127.0.0.1:9/tuxgt.tar.gz");
        match check_app_update().await {
            AppUpdateStatus::Available {
                current,
                tag,
                asset_url,
            } => {
                assert_eq!(current, app_version());
                assert_eq!(tag, "v999.0.0");
                assert_eq!(asset_url, "http://127.0.0.1:9/tuxgt.tar.gz");
            }
            other => panic!("want Available, got {other:?}"),
        }
        std::env::set_var("TUXGT_TEST_UPDATE_TAG", app_version());
        assert!(matches!(
            check_app_update().await,
            AppUpdateStatus::UpToDate { .. }
        ));
        std::env::set_var("TUXGT_TEST_UPDATE_TAG", "v0.0.1");
        assert!(matches!(
            check_app_update().await,
            AppUpdateStatus::UpToDate { .. }
        ));
        std::env::set_var("TUXGT_TEST_UPDATE_TAG", "not-a-version");
        assert!(matches!(
            check_app_update().await,
            AppUpdateStatus::Unknown { .. }
        ));
    }

    #[test]
    fn asset_pick_is_exact_name_or_missing() {
        let asset = |name: &str| GhAsset {
            name: name.into(),
            browser_download_url: format!("https://example.test/{name}"),
        };
        let assets = vec![asset("README.md"), asset("tuxgt.tar.gz")];
        assert_eq!(
            select_app_asset(&assets).as_deref(),
            Some("https://example.test/tuxgt.tar.gz")
        );
        assert_eq!(select_app_asset(&assets[..1]), None);
        assert_eq!(select_app_asset(&[]), None);
    }
}
