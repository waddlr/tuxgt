use gpui_kit::*;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use image::ImageDecoder as _;

pub use tuxgt_core::ArtKind;

use super::fetch::FetchSet;

/// One render per game at a time, across paint kicks and scan renders.
/// Marks clear on completion (success or failure): a mark only dedups
/// concurrent renders, never "rendered once" state.
static RENDER: LazyLock<FetchSet> = LazyLock::new(FetchSet::new);

/// Last finished render per game: its fingerprint file's stamp (len, mtime,
/// or `None` while absent) and when that kick settled. Paint re-kicks a
/// missing thumb when the stamp moved — every render that gets past the
/// fetch stage writes the fingerprint, so a scan render with new sources, an
/// AppID bust, or a pruned dir re-kicks — or when the entry aged out, so art
/// that lands without a scan (Steam writes its librarycache mid-session)
/// still paints. Inside the window an unchanged stamp means paint already
/// asked: a dead source settles instead of looping fetches per paint.
static SETTLED: LazyLock<std::sync::Mutex<HashMap<String, (Stamp, Instant)>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// How long a settled render stands before paint asks again.
const SETTLE_TTL: Duration = Duration::from_secs(60);

/// A fingerprint file's (len, mtime); `None` while the file is absent.
type Stamp = Option<(u64, std::time::SystemTime)>;

/// Stamp of a fingerprint file: (len, mtime), or `None` when it is absent.
fn settle_stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// Whether the last settled render already answers for `stamp` at `now`.
fn render_settled(entry: Option<&(Stamp, Instant)>, stamp: &Stamp, now: Instant) -> bool {
    match entry {
        Some((settled, at)) => settled == stamp && now.saturating_duration_since(*at) < SETTLE_TTL,
        None => false,
    }
}

/// Decoded thumb cache cap. Entries are fixed small thumbs (the largest is
/// the 400x600 grid), so 96 covers the sidebar plus a full Library viewport
/// in a few tens of MB.
pub(crate) const ART_CACHE_CAP: usize = 96;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct ArtKey {
    pub(crate) id: String,
    pub(crate) kind: ArtKind,
}

#[derive(Default)]
pub(crate) struct ArtCache {
    /// key -> (last use tick, image)
    pub(crate) map: std::collections::HashMap<ArtKey, (u64, Arc<RenderImage>)>,
    pub(crate) tick: u64,
    pub(crate) in_flight: std::collections::HashSet<ArtKey>,
}

impl ArtCache {
    fn insert(&mut self, key: ArtKey, img: Arc<RenderImage>) -> Vec<Arc<RenderImage>> {
        self.tick += 1;
        self.map.insert(key, (self.tick, img));
        let mut evicted = Vec::new();
        while self.map.len() > ART_CACHE_CAP {
            let Some(oldest) = self
                .map
                .iter()
                .min_by_key(|(_, (tick, _))| *tick)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some((_, img)) = self.map.remove(&oldest) {
                evicted.push(img);
            }
        }
        evicted
    }

    fn evict_kinds(&mut self, kinds: &[ArtKind]) -> Vec<Arc<RenderImage>> {
        let drop: Vec<ArtKey> = self
            .map
            .keys()
            .filter(|k| kinds.contains(&k.kind))
            .cloned()
            .collect();
        drop.into_iter()
            .filter_map(|k| self.map.remove(&k).map(|(_, img)| img))
            .collect()
    }

    /// Drop every decoded thumb for one game (its thumbs were re-rendered).
    fn evict_game(&mut self, id: &str) -> Vec<Arc<RenderImage>> {
        let drop: Vec<ArtKey> = self.map.keys().filter(|k| k.id == id).cloned().collect();
        drop.into_iter()
            .filter_map(|k| self.map.remove(&k).map(|(_, img)| img))
            .collect()
    }
}

pub(crate) static ART: std::sync::LazyLock<std::sync::Mutex<ArtCache>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(ArtCache::default()));

/// Decoded thumb for one game + kind, if it is in the cache. Touches the
/// entry so the LRU keeps what is on screen — callers must pass viewport
/// rows only (E37), or off-screen entries would evict on-screen covers.
pub fn art_image(id: &str, kind: ArtKind) -> Option<Arc<RenderImage>> {
    let key = ArtKey {
        id: id.to_string(),
        kind,
    };
    let mut cache = ART.lock().expect("art cache");
    cache.tick += 1;
    let tick = cache.tick;
    let (last, img) = cache.map.get_mut(&key)?;
    *last = tick;
    Some(img.clone())
}

/// Decode one thumb off the UI thread, then repaint. A missing thumb kicks
/// the background render instead — once per source state, per [`SETTLED`];
/// paint shows initials until it lands. Callers pass only boxes near the
/// viewport (E37).
pub fn kick_art_decode(id: &str, kind: ArtKind, view: Entity<super::super::Shell>, cx: &App) {
    let path = tuxgt_core::thumb_file(&tuxgt_core::data_dir(), id, kind);
    if !path.is_file() {
        let stamp = settle_stamp(&tuxgt_core::art_fingerprint_file(
            &tuxgt_core::data_dir(),
            id,
        ));
        let settled = {
            let map = SETTLED.lock().expect("art settled");
            render_settled(map.get(id), &stamp, Instant::now())
        };
        if settled {
            return;
        }
        kick_art_render(id, view, cx);
        return;
    }
    let key = ArtKey {
        id: id.to_string(),
        kind,
    };
    {
        let mut cache = ART.lock().expect("art cache");
        if cache.map.contains_key(&key) || !cache.in_flight.insert(key.clone()) {
            return;
        }
    }
    cx.spawn(async move |cx| {
        let decoded = cx
            .background_spawn(async move { decode_cover(&path) })
            .await;
        let _ = cx.update(|cx| {
            let id = key.id.clone();
            let (ok, evicted) = {
                let mut cache = ART.lock().expect("art cache");
                cache.in_flight.remove(&key);
                match decoded {
                    Some(img) => (true, cache.insert(key, img)),
                    None => (false, Vec::new()),
                }
            };
            for img in evicted {
                cx.drop_image(img, None);
            }
            // A failed decode settles: the render kick (which landed or will
            // land the thumb) owns the repaint, so a corrupt file never
            // loops repaints.
            if ok {
                tuxgt_core::touch_game_art(&tuxgt_core::data_dir(), &id);
                view.update(cx, |_, cx| cx.notify());
            }
        });
    })
    .detach();
}

/// Blocking one-game render shared by the background shells below (the
/// spawn shells legitimately differ: `App` vs `Context`).
fn render_one_sync(id: &str) -> tuxgt_core::Result<bool> {
    super::super::rt_block(async {
        let dir = tuxgt_core::data_dir();
        let pool = tuxgt_core::open_db_shared(&dir).await?;
        tuxgt_core::render_game_art(&pool, &dir, id).await
    })
}

/// Render one game's thumbs in the background, then repaint. Deduped per
/// game against concurrent kicks and the scan render; every outcome settles
/// the fingerprint stamp, so a failed or empty render is not retried from
/// paint (the next scan render is the retry).
pub fn kick_art_render(id: &str, view: Entity<super::super::Shell>, cx: &App) {
    if !RENDER.mark(id) {
        return;
    }
    let owned = id.to_string();
    let for_bg = owned.clone();
    cx.spawn(async move |cx| {
        let result = cx
            .background_spawn(async move { render_one_sync(&for_bg) })
            .await;
        // Marks dedup concurrent renders only; the thumb file (or its
        // absence) drives the next kick, so always unmark. Only a render
        // that changed the art dir evicts + repaints: a fingerprint skip
        // leaves the cache correct, and notifying on it would re-kick
        // paint-driven renders forever on artless games.
        RENDER.unmark(&owned);
        SETTLED.lock().expect("art settled").insert(
            owned.clone(),
            (
                settle_stamp(&tuxgt_core::art_fingerprint_file(
                    &tuxgt_core::data_dir(),
                    &owned,
                )),
                Instant::now(),
            ),
        );
        let rendered = match result {
            Ok(rendered) => rendered,
            Err(e) => {
                tracing::warn!(game = %owned, error = %e, "art render failed");
                return;
            }
        };
        if !rendered {
            return;
        }
        let _ = cx.update(|cx| {
            let evicted = ART.lock().expect("art cache").evict_game(&owned);
            drop_images(evicted, cx);
            view.update(cx, |_, cx| cx.notify());
        });
    })
    .detach();
}

/// GPU-side release for images already removed from [`ART`]. Callers pass
/// App-level `cx` from outside any window update: with the window taken
/// out of `App.windows`, `drop_image(_, None)` iterates zero windows and
/// silently keeps every tile.
fn drop_images(images: Vec<Arc<RenderImage>>, cx: &mut App) {
    for img in images {
        cx.drop_image(img, None);
    }
}

impl super::super::Shell {
    /// Evict decoded thumbs of kinds the current page never paints. Called
    /// from the root render (every page, every frame): the sidebar keeps
    /// its side/rail thumbs everywhere, grid/list live only on the Library
    /// page, the hero only on the game page.
    pub(crate) fn evict_offpage_art(&self, cx: &mut Context<Self>) {
        use super::super::Nav;
        use tuxgt_core::ArtKind::{Grid, Hero, List};
        let kinds: &[ArtKind] = match self.nav {
            Nav::Library => &[Hero],
            Nav::Game => &[Grid, List],
            Nav::Settings => &[Grid, List, Hero],
        };
        self.evict_art(kinds, cx);
    }

    /// Evict decoded thumbs of the given kinds now (game switch drops the
    /// previous hero; the root render drops off-page kinds every frame).
    /// The map removal is immediate; the GPU release runs deferred on
    /// App-level `cx`, where the window is back in `App.windows`.
    pub(crate) fn evict_art(&self, kinds: &[ArtKind], cx: &mut Context<Self>) {
        let evicted = ART.lock().expect("art cache").evict_kinds(kinds);
        if evicted.is_empty() {
            return;
        }
        cx.spawn(async move |_, cx| {
            cx.update(|cx| drop_images(evicted, cx));
        })
        .detach();
    }

    /// Render every game's thumbs after scan, in one background task with a
    /// single pool. Fingerprints skip unchanged games; re-rendered games
    /// evict their stale decodes, then one repaint (visible rows also kick
    /// individually at paint with their own repaints).
    pub(crate) fn spawn_art_render_all(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.index.iter().map(|e| e.id.clone()).collect();
        cx.spawn(async move |this, cx| {
            let rendered: Vec<String> = cx
                .background_spawn(async move {
                    match super::super::rt_block(async move {
                        let dir = tuxgt_core::data_dir();
                        let pool = tuxgt_core::open_db_shared(&dir).await?;
                        let mut rendered = Vec::new();
                        for id in &ids {
                            if !RENDER.mark(id) {
                                continue;
                            }
                            let r = tuxgt_core::render_game_art(&pool, &dir, id).await;
                            RENDER.unmark(id);
                            match r {
                                Ok(true) => rendered.push(id.clone()),
                                Ok(false) => {}
                                Err(e) => {
                                    tracing::warn!(game = %id, error = %e, "art render failed");
                                }
                            }
                        }
                        Ok::<_, tuxgt_core::Error>(rendered)
                    }) {
                        Ok(rendered) => rendered,
                        Err(e) => {
                            tracing::warn!(error = %e, "art render-all failed");
                            Vec::new()
                        }
                    }
                })
                .await;
            if rendered.is_empty() {
                return;
            }
            let _ = cx.update(|cx| {
                let mut cache = ART.lock().expect("art cache");
                let mut evicted = Vec::new();
                for id in &rendered {
                    evicted.extend(cache.evict_game(id));
                }
                drop(cache);
                drop_images(evicted, cx);
            });
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// Render one game now (metadata refresh, AppID save): on a render that
    /// wrote thumbs, evict its stale decodes, then repaint. A fingerprint
    /// skip leaves the cache correct and settles without a repaint.
    pub(crate) fn spawn_art_render_one(&mut self, id: &str, cx: &mut Context<Self>) {
        if !RENDER.mark(id) {
            return;
        }
        let owned = id.to_string();
        let for_bg = owned.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { render_one_sync(&for_bg) })
                .await;
            RENDER.unmark(&owned);
            let rendered = match result {
                Ok(rendered) => rendered,
                Err(e) => {
                    tracing::warn!(game = %owned, error = %e, "art render failed");
                    return;
                }
            };
            if !rendered {
                return;
            }
            let _ = cx.update(|cx| {
                let evicted = ART.lock().expect("art cache").evict_game(&owned);
                drop_images(evicted, cx);
            });
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }
}

/// Decode `path` full-size (thumbs are pre-sized at render), convert to
/// the BGRA the renderer expects, and wrap it for `img()`.
pub(crate) fn decode_cover(path: &std::path::Path) -> Option<Arc<RenderImage>> {
    let reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
    img.apply_orientation(orientation);
    let mut buf = img.into_rgba8();
    for pixel in buf.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buf)])))
}

#[cfg(test)]
mod tests {
    use super::render_settled;
    use super::settle_stamp;
    use super::ArtCache;
    use super::ArtKey;
    use super::SETTLE_TTL;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime};
    use tuxgt_core::ArtKind;

    fn image() -> Arc<gpui_kit::RenderImage> {
        let buf = image::RgbaImage::new(1, 1);
        Arc::new(gpui_kit::RenderImage::new(vec![image::Frame::new(buf)]))
    }

    #[test]
    fn render_settled_only_inside_the_window() {
        let now = Instant::now();
        let stamp = Some((10, SystemTime::UNIX_EPOCH));
        let other = Some((11, SystemTime::UNIX_EPOCH));
        // Nothing settled yet.
        assert!(!render_settled(None, &stamp, now));
        // Fresh entry, same stamp: paint has nothing to ask for.
        assert!(render_settled(Some(&(stamp, now)), &stamp, now));
        // Moved stamp (scan render, AppID bust, pruned dir): ask again.
        assert!(!render_settled(Some(&(stamp, now)), &other, now));
        // Aged out: art may have landed without a scan, so ask again.
        let old = now - SETTLE_TTL - Duration::from_secs(1);
        assert!(!render_settled(Some(&(stamp, old)), &stamp, now));
        // Absent fingerprint, settled: same rule, no thumb to show.
        assert!(render_settled(Some(&(None, now)), &None, now));
    }

    #[test]
    fn evict_kinds_drops_only_those_kinds() {
        let mut cache = ArtCache::default();
        for kind in [ArtKind::Grid, ArtKind::Rail, ArtKind::Hero] {
            cache.insert(
                ArtKey {
                    id: "steam::1".into(),
                    kind,
                },
                image(),
            );
        }
        let evicted = cache.evict_kinds(&[ArtKind::Grid, ArtKind::List]);
        assert_eq!(evicted.len(), 1);
        assert_eq!(cache.map.len(), 2);
        assert!(cache.map.keys().all(|k| k.kind != ArtKind::Grid));
    }

    #[test]
    fn settle_stamp_moves_with_the_fingerprint() {
        let dir = std::env::temp_dir().join(format!("tuxgt-artstamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fp = dir.join("src");
        // Absent fingerprint: nothing settled, so paint may kick a render.
        assert_eq!(settle_stamp(&fp), None);
        std::fs::write(&fp, b"cover=-\n").unwrap();
        let first = settle_stamp(&fp).expect("stamp after write");
        // A rewritten fingerprint (scan render with new sources, AppID bust)
        // moves the stamp, which is what re-allows the paint kick.
        std::fs::write(&fp, b"cover=file:/a|1|2\n").unwrap();
        assert_ne!(settle_stamp(&fp), Some(first));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn evict_game_drops_only_that_game() {
        let mut cache = ArtCache::default();
        for id in ["steam::1", "steam::2"] {
            for kind in [ArtKind::Grid, ArtKind::Rail] {
                cache.insert(
                    ArtKey {
                        id: id.into(),
                        kind,
                    },
                    image(),
                );
            }
        }
        let evicted = cache.evict_game("steam::1");
        assert_eq!(evicted.len(), 2);
        assert_eq!(cache.map.len(), 2);
        assert!(cache.map.keys().all(|k| k.id == "steam::2"));
    }
}
