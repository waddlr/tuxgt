//! Thumb renderer: fixed-size writes from resolved sources.

use std::fs;
use std::path::{Path, PathBuf};

use image::{GenericImageView, ImageFormat};
use sqlx::SqlitePool;

use super::super::{
    art_dir, art_file, drop_game_hero, drop_game_icon, game_id_safe, hero_file, icon_file,
    land_art, land_hero, land_icon,
};
use super::sources::{art_fingerprint, fetch_original, fingerprint_file, grid_urls, is_http};
use super::{thumb_file, ArtKind};
use crate::{atomic_write, Error, Result};

/// Center-crop to the `aw:ah` aspect, then scale down to fit `max_box`
/// (never upscale) — or to the exact canvas when the kind names one.
/// Paint Cover-crops the same center, so thumbs match.
fn fit_thumb(img: &image::DynamicImage, kind: ArtKind) -> image::DynamicImage {
    let (w, h) = img.dimensions();
    let img = match kind.crop_aspect() {
        Some((aw, ah)) if w > 0 && h > 0 => {
            // Largest centered box with the target aspect.
            let (mut cw, mut ch) = (w, h);
            if w * ah > h * aw {
                cw = (h as u64 * aw as u64 / ah as u64).max(1).min(w as u64) as u32;
            } else {
                ch = (w as u64 * ah as u64 / aw as u64).max(1).min(h as u64) as u32;
            }
            img.crop_imm((w - cw) / 2, (h - ch) / 2, cw, ch)
        }
        _ => img.clone(),
    };
    if let Some((tw, th)) = kind.exact_size() {
        if img.dimensions() != (tw, th) {
            return img.resize_exact(tw, th, image::imageops::FilterType::Triangle);
        }
        return img;
    }
    let (mw, mh) = kind.max_box();
    let (w, h) = img.dimensions();
    if w <= mw && h <= mh {
        return img;
    }
    let s = (mw as f32 / w as f32).min(mh as f32 / h as f32);
    let (tw, th) = (
        ((w as f32 * s).round() as u32).max(1),
        ((h as f32 * s).round() as u32).max(1),
    );
    img.resize_exact(tw, th, image::imageops::FilterType::Triangle)
}

fn write_thumb(dest: &Path, kind: ArtKind, img: &image::DynamicImage) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = Vec::new();
    match kind.format() {
        ImageFormat::Jpeg => {
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85);
            enc.encode_image(img)
                .map_err(|e| Error::Cache(e.to_string()))?;
        }
        _ => img
            .write_to(&mut std::io::Cursor::new(&mut bytes), kind.format())
            .map_err(|e| Error::Cache(e.to_string()))?,
    }
    atomic_write(dest, &bytes)?;
    Ok(())
}

/// Drop one kind's thumb. Returns whether a file was there to drop.
fn remove_thumb(data_dir: &Path, game_id: &str, kind: ArtKind) -> bool {
    fs::remove_file(thumb_file(data_dir, game_id, kind)).is_ok()
}

/// AppID save: drop the hero/icon originals, all thumbs, and the
/// fingerprint. The cover original stays (AppID-independent), so the
/// re-render refetches only what the AppID could have moved.
pub fn bust_game_art_for_appid(data_dir: &Path, game_id: &str) {
    drop_game_hero(data_dir, game_id);
    drop_game_icon(data_dir, game_id);
    for kind in ArtKind::ALL {
        remove_thumb(data_dir, game_id, kind);
    }
    let _ = fs::remove_file(fingerprint_file(data_dir, game_id));
}

/// Mark one game's art dir recently used (decode = genuine use), so the
/// dir-cap eviction stays least-recently-used rather than least-rendered.
pub fn touch_game_art(data_dir: &Path, game_id: &str) {
    let dir = art_dir(data_dir).join(game_id_safe(game_id));
    if !dir.is_dir() {
        return;
    }
    let _ = fs::File::open(&dir).and_then(|f| f.set_modified(std::time::SystemTime::now()));
}

/// Render one game's thumbs. Skips when the fingerprint matches (sources
/// unchanged). Returns whether the art dir changed (thumbs written or
/// dropped). A missing source drops its thumbs (initials at paint); a failed
/// fetch errors so the caller retries later, while an undecodable file is
/// skipped like a missing one (its mtime busts the fingerprint if it ever
/// changes).
pub async fn render_game_art(pool: &SqlitePool, data_dir: &Path, game_id: &str) -> Result<bool> {
    let Some(row) = crate::game_row_by_id(pool, game_id).await? else {
        return Ok(false);
    };
    let meta = crate::cached_metadata_for(pool, game_id)
        .await
        .unwrap_or_default();
    let (hero_url, icon_url) = grid_urls(&meta, game_id);
    // Rail fallback matches the old GUI `rail_icon`: the Steam client icon
    // resolves for steam-manager games only (an AppID overlay on another
    // manager drives metadata, never the icon).
    let steam_icon = if row.manager == "steam" {
        row.resolved_appid()
            .and_then(|a| a.parse::<u32>().ok())
            .and_then(crate::steam_icon_for_appid)
    } else {
        None
    };
    let fp = art_fingerprint(
        row.cover_path.as_deref(),
        row.header_path.as_deref(),
        steam_icon.as_deref(),
        hero_url.as_deref(),
        icon_url.as_deref(),
    );
    if fs::read_to_string(fingerprint_file(data_dir, game_id))
        .map(|s| s == fp)
        .unwrap_or(false)
    {
        return Ok(false);
    }

    // Cover original: local file, else the fetched cover (only when a
    // provider URL names it — a stale cache file without a URL stays dead,
    // exactly like the old paint check).
    let cover_url = row.art_url().map(str::to_string);
    let mut cover: Option<PathBuf> = row.local_art();
    if cover.is_none() {
        if let Some(url) = cover_url {
            let dest = art_file(data_dir, game_id);
            cover = Some(if dest.is_file() {
                dest
            } else {
                fetch_original(data_dir, game_id, &url, land_art, dest).await?
            });
        }
    }
    // Hero original: header file, else the fetched hero.
    let header_is_file = row
        .header_path
        .as_deref()
        .is_some_and(|s| Path::new(s).is_file());
    let mut hero: Option<PathBuf> = header_is_file
        .then(|| row.header_path.clone())
        .flatten()
        .map(PathBuf::from);
    if hero.is_none() {
        let dest = hero_file(data_dir, game_id);
        if dest.is_file() {
            hero = Some(dest);
        } else if let Some(url) = row
            .header_path
            .as_deref()
            .filter(|s| is_http(s))
            .map(str::to_string)
            .or(hero_url.clone())
        {
            hero = Some(fetch_original(data_dir, game_id, &url, land_hero, dest).await?);
        }
    }
    // Rail original: cover, Steam icon, fetched icon, in that order.
    let mut rail: Option<PathBuf> = cover.clone().or_else(|| steam_icon.clone());
    if rail.is_none() {
        let dest = icon_file(data_dir, game_id);
        if dest.is_file() {
            rail = Some(dest);
        } else if let Some(url) = icon_url {
            rail = Some(fetch_original(data_dir, game_id, &url, land_icon, dest).await?);
        }
    }

    let mut changed = false;
    for (original, kinds) in [
        (
            cover,
            [ArtKind::Grid, ArtKind::List, ArtKind::Side].as_slice(),
        ),
        (rail, [ArtKind::Rail].as_slice()),
        (hero, [ArtKind::Hero].as_slice()),
    ] {
        let img = original.and_then(|path| {
            image::ImageReader::open(&path)
                .ok()
                .and_then(|r| r.with_guessed_format().ok())
                .and_then(|r| r.decode().ok())
        });
        match img {
            Some(img) => {
                for kind in kinds {
                    let dest = thumb_file(data_dir, game_id, *kind);
                    write_thumb(&dest, *kind, &fit_thumb(&img, *kind))?;
                    changed = true;
                }
            }
            // No source (or an undecodable one): these kinds have no art, so
            // a thumb from an earlier source must go — otherwise paint keeps
            // showing art whose source is gone.
            None => {
                for kind in kinds {
                    changed |= remove_thumb(data_dir, game_id, *kind);
                }
            }
        }
    }
    if let Some(parent) = fingerprint_file(data_dir, game_id).parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(&fingerprint_file(data_dir, game_id), fp.as_bytes())?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{seed_game, SeedGame};

    #[test]
    fn fit_thumb_crops_center_and_never_upscales() {
        // Wide 800x400 → grid 2:3 center crop (266x400) → fits 400x600 box.
        let wide = image::DynamicImage::new_rgb8(800, 400);
        let thumb = fit_thumb(&wide, ArtKind::Grid);
        assert_eq!((thumb.width(), thumb.height()), (266, 400));
        // Tall 300x900 → grid crop (300x450).
        let tall = image::DynamicImage::new_rgb8(300, 900);
        let thumb = fit_thumb(&tall, ArtKind::Grid);
        assert_eq!((thumb.width(), thumb.height()), (300, 450));
        // Small image stays put (no upscale).
        let small = image::DynamicImage::new_rgb8(32, 32);
        let thumb = fit_thumb(&small, ArtKind::Rail);
        assert_eq!((thumb.width(), thumb.height()), (32, 32));
        // Hero is exact-canvas, not fit-in-box: see
        // `hero_canvas_is_fixed_across_aspects`.
    }

    #[test]
    fn hero_canvas_is_fixed_across_aspects() {
        // Pool reuse needs byte-identical uploads: every source aspect
        // lands on the same 3:1 canvas.
        for (w, h) in [(3840, 1000), (1600, 1600), (600, 900), (1920, 640)] {
            let thumb = fit_thumb(&image::DynamicImage::new_rgb8(w, h), ArtKind::Hero);
            assert_eq!((thumb.width(), thumb.height()), (1920, 640));
        }
    }

    #[test]
    fn fingerprint_carries_render_geometry() {
        // The marker is the per-kind geometry, not a hand-bumped literal: a
        // constant marker (the `render=v2` this replaced) fails here, and so
        // does a geometry input the marker forgets to cover.
        let fp = art_fingerprint(None, None, None, None, None);
        for kind in ArtKind::ALL {
            for part in [
                format!("{:?}", kind.max_box()),
                format!("{:?}", kind.exact_size()),
                format!("{:?}", kind.crop_aspect()),
                format!("{:?}", kind.format()),
            ] {
                assert!(fp.contains(&part), "{kind:?} missing {part}");
            }
        }
    }

    #[test]
    fn fingerprint_busts_on_source_change() {
        let dir = std::env::temp_dir().join(format!("tuxgt-artfp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let cover = dir.join("cover.jpg");
        fs::write(&cover, b"fake").unwrap();
        let base = art_fingerprint(
            Some(&cover.to_string_lossy()),
            None,
            None,
            Some("https://x/hero.png"),
            None,
        );
        // Same inputs → same fingerprint.
        assert_eq!(
            base,
            art_fingerprint(
                Some(&cover.to_string_lossy()),
                None,
                None,
                Some("https://x/hero.png"),
                None,
            )
        );
        // New bytes → bust. URL change → bust. Icon URL added → bust. Each
        // case moves one input only, so the assertion cannot pass for the
        // wrong reason.
        fs::write(&cover, b"fake-changed").unwrap();
        let changed = art_fingerprint(
            Some(&cover.to_string_lossy()),
            None,
            None,
            Some("https://x/hero.png"),
            None,
        );
        assert_ne!(base, changed);
        assert_ne!(
            base,
            art_fingerprint(
                Some(&cover.to_string_lossy()),
                None,
                None,
                Some("https://x/other.png"),
                None
            )
        );
        assert_ne!(
            base,
            art_fingerprint(
                Some(&cover.to_string_lossy()),
                None,
                None,
                Some("https://x/hero.png"),
                Some("https://x/icon.png"),
            )
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn render_writes_thumbs_then_skips() {
        let dir = std::env::temp_dir().join(format!("tuxgt-artrender-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let pool = crate::open_db(&dir).await.unwrap();
        // 600x900 local cover: grid keeps a 2:3 crop inside 400x600.
        let cover = dir.join("cover.png");
        image::DynamicImage::new_rgb8(600, 900)
            .save_with_format(&cover, ImageFormat::Png)
            .unwrap();
        seed_game(
            &pool,
            SeedGame {
                id: "steam::1",
                manager: "steam",
                name: Some("Doom"),
                ..Default::default()
            },
        )
        .await;
        sqlx::query("UPDATE games SET cover_path = ? WHERE id = 'steam::1'")
            .bind(cover.to_string_lossy().into_owned())
            .execute(&pool)
            .await
            .unwrap();
        assert!(render_game_art(&pool, &dir, "steam::1").await.unwrap());
        for kind in [ArtKind::Grid, ArtKind::List, ArtKind::Side] {
            let p = thumb_file(&dir, "steam::1", kind);
            assert!(p.is_file(), "{}", p.display());
        }
        // Rail falls back to the cover; hero has no source, so initials
        // paint until one lands.
        assert!(thumb_file(&dir, "steam::1", ArtKind::Rail).is_file());
        assert!(!thumb_file(&dir, "steam::1", ArtKind::Hero).is_file());
        let grid = image::open(thumb_file(&dir, "steam::1", ArtKind::Grid)).unwrap();
        assert_eq!((grid.width(), grid.height()), (400, 600));
        // Second render: fingerprint matches, nothing rewritten.
        assert!(!render_game_art(&pool, &dir, "steam::1").await.unwrap());
        // Source gone: the stale thumbs drop (paint falls back to initials)
        // instead of being kept by the fingerprint skip.
        sqlx::query("UPDATE games SET cover_path = NULL WHERE id = 'steam::1'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(render_game_art(&pool, &dir, "steam::1").await.unwrap());
        for kind in ArtKind::ALL {
            let p = thumb_file(&dir, "steam::1", kind);
            assert!(!p.is_file(), "{} kept", p.display());
        }
        // Unknown game: no render, no error.
        assert!(!render_game_art(&pool, &dir, "steam::nope").await.unwrap());
        // AppID bust drops thumbs + fingerprint; the cover original stays.
        bust_game_art_for_appid(&dir, "steam::1");
        assert!(!thumb_file(&dir, "steam::1", ArtKind::Grid).is_file());
        assert!(cover.is_file());
        pool.close().await;
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[test]
    fn touch_marks_art_dir_used() {
        let dir = std::env::temp_dir().join(format!("tuxgt-arttouch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        // Missing dir: silent no-op.
        touch_game_art(&dir, "steam::1");
        let game_dir = super::super::art_dir(&dir).join("steam__1");
        fs::create_dir_all(&game_dir).unwrap();
        touch_game_art(&dir, "steam::1");
        let mtime = fs::metadata(&game_dir).unwrap().modified().unwrap();
        assert!(mtime.elapsed().unwrap().as_secs() < 60);
        let _ = fs::remove_dir_all(&dir);
    }
}
