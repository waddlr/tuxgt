#define _GNU_SOURCE
#include <fcntl.h>
#include <sched.h>
#include "tuxgt-launcher.h"

// Private sentinel for the status Error field: entries skipped because the
// staged bytes diverged from the depot. Not a Windows error; Loaded counts
// fresh loads only, so a stale skip reads Loaded=x/y with this Error.
#define TUXGT_ESTALE ((unsigned long)0xE57A1E)

loadlibraryw_fn g_LoadLibraryW;
loadlibraryw_fn g_orig_llw;
void *__attribute__((ms_abi)) (*g_orig_llew)(const uint16_t *, void *, uint32_t);
int g_in_ll;
unsigned long __attribute__((ms_abi)) (*g_orig_gtc)(void);
unsigned long long __attribute__((ms_abi)) (*g_orig_gtc64)(void);
int __attribute__((ms_abi)) (*g_orig_qpc)(void *);
void __attribute__((ms_abi)) (*g_orig_sleep)(unsigned long);
unsigned long __attribute__((ms_abi)) (*g_orig_sleepex)(unsigned long, int);
unsigned long __attribute__((ms_abi)) (*g_GetLastError)(void);
int __attribute__((ms_abi)) (*g_SetEnvW)(const uint16_t *, const uint16_t *);
int __attribute__((ms_abi)) (*g_FlushIC)(void *proc, const void *addr, size_t n);
unsigned long __attribute__((ms_abi)) (*g_GetModuleFileNameW)(void *mod, uint16_t *buf, unsigned long n);
int g_cfg_done;
struct game_cfg g_cfg;

// ReShade compat quirk, triggered by dest basename (never by config keys):
// stock-named ReShade loads refuse LoadLibrary unless <base>/ReShade.ini
// exists, and read config/log paths relative to it. Proxy-named loads
// (e.g. d3d12.dll) need neither: the DLL dir already is the per-game base,
// and the stock-stem loading check does not apply.
int is_reshade_name(const char *dest) {
    const char *b = strrchr(dest, '/');
    b = b ? b + 1 : dest;
    return strcasecmp(b, "ReShade64.dll") == 0 || strcasecmp(b, "ReShade32.dll") == 0;
}

// Ensure <base>/ReShade.ini exists (never overwrite a real config) and point
// ReShade's base override at the per-game base, Unix side and PE side
// (Unix setenv is NOT visible to PE GetEnvironmentVariableW).
void reshade_quirk(void) {
    char ini[PATH_MAX + 16];
    snprintf(ini, sizeof(ini), "%s/ReShade.ini", g_cfg.home_unix);
    FILE *f = fopen(ini, "r");
    if (f) {
        fclose(f);
    } else {
        f = fopen(ini, "w");
        if (f)
            fclose(f);
        else
            log_msg("ReShade.ini touch failed");
    }
    char zbase[PATH_MAX + 8];
    unix_to_z(g_cfg.home_unix, zbase, sizeof(zbase));
    static uint16_t wbase[PATH_MAX + 8];
    if (!to_utf16(zbase, wbase, PATH_MAX + 8)) {
        log_msg("utf8 path conversion failed");
        return;
    }
    setenv("RESHADE_BASE_PATH_OVERRIDE", zbase, 1);
    if (g_SetEnvW) {
        static uint16_t wname[64];
        if (to_utf16("RESHADE_BASE_PATH_OVERRIDE", wname, 64))
            g_SetEnvW(wname, wbase);
        else
            log_msg("utf8 path conversion failed");
    }
}

// LoadDLL entries in list order from the staged per-game base. Counts loads
// for the status file and returns the last handle (NULL when the list is
// empty); appends "dest=handle" lines after the "dest=result" stage lines.
void load_loaddll(char *rep, size_t n, unsigned *nok, unsigned *ntotal, void **last_h,
                         unsigned long *last_err) {
    *nok = 0;
    *ntotal = 0;
    *last_h = NULL;
    *last_err = 0;
    if (!g_cfg.loaddll[0]) return;
    loadlibraryw_fn ll = g_orig_llw ? g_orig_llw : g_LoadLibraryW;
    if (!ll) return;
    char list[TUXGT_LIST_BUDGET];
    snprintf(list, sizeof(list), "%s", g_cfg.loaddll);
    int saved = g_in_ll;
    g_in_ll = 1;
    char *save = NULL;
    for (char *tok = strtok_r(list, ",", &save); tok; tok = strtok_r(NULL, ",", &save)) {
        while (*tok == ' ' || *tok == '\t') tok++;
        char *end = tok + strlen(tok);
        while (end > tok && (end[-1] == ' ' || end[-1] == '\t')) *--end = 0;
        char dest[PATH_MAX], src[PATH_MAX];
        int is_tree = 0;
        if (!split_mapping(tok, dest, sizeof(dest), src, sizeof(src), &is_tree) || is_tree) continue;
        // Only *.dll entries are loadable; other staged files (ini, logs)
        // are data. Case-insensitive.
        size_t dl = strlen(dest);
        if (dl < 5 || strcasecmp(dest + dl - 4, ".dll") != 0) continue;
        if (is_reshade_name(dest)) reshade_quirk();
        uint16_t w[PATH_MAX];
        void *h = NULL;
        char p[PATH_MAX + 8], z[PATH_MAX + 16];
        snprintf(p, sizeof(p), "%s/%s", g_cfg.home_unix, dest);
        unix_to_z(p, z, sizeof(z));
        // Verify the staged bytes still match the depot: loads run outside
        // the stage lock, so a concurrent process may have pruned or
        // replaced them after we staged. Skip loudly instead of handing the
        // game a half-staged DLL. Size-checked before hashing.
        int stale = 0;
        if (g_cfg.depot_unix[0]) {
            char dsrc[PATH_MAX];
            snprintf(dsrc, sizeof(dsrc), "%s/%s", g_cfg.depot_unix, src);
            long ss = file_size(dsrc);
            if (ss < 0) {
                char vmsg[PATH_MAX + 64];
                snprintf(vmsg, sizeof(vmsg), "load skipped, depot entry missing: %s", dest);
                log_msg(vmsg);
                stale = 1;
            } else if (ss != file_size(p) || !same_digest(dsrc, p)) {
                char vmsg[PATH_MAX + 64];
                snprintf(vmsg, sizeof(vmsg), "load skipped, dest stale vs depot: %s", dest);
                log_msg(vmsg);
                stale = 1;
            }
            if (stale) *last_err = TUXGT_ESTALE;
        }
        if (!stale && to_utf16(z, w, PATH_MAX)) h = ll(w);
        if (!stale && !h && !strchr(dest, '/')) {
            if (to_utf16(dest, w, PATH_MAX)) h = ll(w);
        }
        (*ntotal)++;
        if (h) {
            (*nok)++;
            *last_h = h;
        } else if (!stale) {
            *last_err = g_GetLastError ? g_GetLastError() : 0;
            char dbg[PATH_MAX + 64];
            snprintf(dbg, sizeof(dbg), "load failed: %s err=%lu", dest, *last_err);
            log_debug(dbg);
        }
        char msg[512];
        snprintf(msg, sizeof(msg), "side DLL %s -> %p", dest, h);
        log_msg(msg);
        size_t used = strlen(rep);
        if (used + strlen(dest) + 20 < n) {
            snprintf(rep + used, n - used, "%s=%p\n", dest, h);
        }
    }
    g_in_ll = saved;
}

// Status file in the per-game base: loader writes, tuxgt addon reads.
// Via temp + rename so a concurrent reader (second game process sharing the
// base) never sees a torn file. [LoadDLL] stays last: the addon gates its read
// on that section being present.
void write_status(const char *result, void *handle, unsigned lok, unsigned ltotal, unsigned long err,
                         const char *irep, const char *drep) {
    char path[PATH_MAX + 32], tmp[PATH_MAX + 40];
    snprintf(path, sizeof(path), "%s/tuxgt-launcher-status.ini", g_cfg.home_unix);
    snprintf(tmp, sizeof(tmp), "%s.tmp", path);
    FILE *f = fopen(tmp, "w");
    if (!f) return;
    fprintf(f, "[Load]\nType=%s\nBase=%s\nResult=%s\nHandle=%p\nLoaded=%u/%u\nError=%lu\n[IncludeFile]\n%s[LoadDLL]\n%s",
            g_cfg.type, g_cfg.home_unix, result, handle, lok, ltotal, err, irep, drep);
    fclose(f);
    rename(tmp, path);
}

// Staging runs on exactly one thread: `once` 0 idle, 1 busy, 2 done.
// `stager` is that thread's tid: DllMain runs synchronously inside our own
// LoadLibrary calls, so gated calls re-entering from it must not spin on
// the once flag we hold ourselves.
static int once = 0;
static pid_t stager = 0;

// Spin while another thread stages: returning early would run the caller on
// a half-staged base. The stager never waits on spinners, and the stager
// itself never spins, so no deadlock either way.
void load_wait(void) {
    while (__atomic_load_n(&once, __ATOMIC_ACQUIRE) == 1 && stager != gettid()) sched_yield();
}

// fcntl record lock, not flock: record locks are not inherited across fork,
// so forked-without-exec wine helpers can never pin the stage lock.
// O_CLOEXEC still keeps the fd itself out of exec'd children. Returns 1 on
// lock held (fd_out set), 0 with a loud log otherwise; ~30s deadline, then
// the caller proceeds unlocked and must skip prune on that path.
static int stage_lock(const char *home, int *fd_out) {
    *fd_out = -1;
    char lockpath[PATH_MAX + 32];
    snprintf(lockpath, sizeof(lockpath), "%s/tuxgt-launcher.lock", home);
    int fd = open(lockpath, O_CREAT | O_RDWR | O_CLOEXEC, 0644);
    if (fd < 0) {
        log_msg("stage lock open failed, proceeding unlocked");
        return 0;
    }
    struct flock fl;
    memset(&fl, 0, sizeof(fl));
    fl.l_type = F_WRLCK;
    fl.l_whence = SEEK_SET;
    if (fcntl(fd, F_SETLK, &fl) == 0) {
        *fd_out = fd;
        return 1;
    }
    if (errno != EACCES && errno != EAGAIN) {
        log_msg("stage lock unsupported, proceeding unlocked");
        close(fd);
        return 0;
    }
    log_msg("stage lock contested, waiting");
    time_t end = time(NULL) + 30;
    struct timespec ts = {0, 50000000};
    for (;;) {
        nanosleep(&ts, NULL);
        if (fcntl(fd, F_SETLK, &fl) == 0) {
            *fd_out = fd;
            return 1;
        }
        if ((errno != EACCES && errno != EAGAIN) || time(NULL) >= end) break;
    }
    log_msg("stage lock wait timed out, proceeding unlocked");
    close(fd);
    return 0;
}

static void stage_unlock(int fd) {
    if (fd < 0) return;
    struct flock fl;
    memset(&fl, 0, sizeof(fl));
    fl.l_type = F_UNLCK;
    fl.l_whence = SEEK_SET;
    fcntl(fd, F_SETLK, &fl);
    close(fd);
}

void load_once(const char *via) {
    load_wait();
    if (__atomic_load_n(&once, __ATOMIC_ACQUIRE) != 0) return;
    if (!g_cfg_done) return;
    if (!__sync_bool_compare_and_swap(&once, 0, 1)) {
        load_wait();
        return;
    }
    stager = gettid();
    if (g_GetModuleFileNameW) {
        uint16_t wbuf[PATH_MAX];
        unsigned long n = g_GetModuleFileNameW(NULL, wbuf, PATH_MAX);
        if (n > 0 && n < PATH_MAX) {
            char path[PATH_MAX];
            unsigned long i;
            for (i = 0; i < n && i + 1 < (unsigned long)PATH_MAX; i++) {
                uint16_t c = wbuf[i];
                if (!c) break;
                path[i] = (c < 128) ? (char)c : '?';
            }
            path[i] = 0;
            if (path[0]) {
                char stem[256];
                exe_stem(path, stem, sizeof(stem));
                resolve_cfg(stem, &g_cfg);
                if (!g_cfg.ok) {
                    __atomic_store_n(&once, 2, __ATOMIC_RELEASE);
                    return;
                }
            }
        }
    }
    if (!g_cfg.ok) {
        __atomic_store_n(&once, 2, __ATOMIC_RELEASE);
        return;
    }
    char irep[TUXGT_LIST_BUDGET] = {0}, drep[TUXGT_LIST_BUDGET] = {0};
    if (!mkdir_p(g_cfg.home_unix)) {
        log_msg("mkdir per-game base failed");
        __atomic_store_n(&once, 0, __ATOMIC_RELEASE);
        return;
    }
    // Window 1: manifest + stage under the record lock. Released before any
    // LoadLibrary runs: game code (DllMain) must never execute under it.
    int lfd = -1;
    stage_lock(g_cfg.home_unix, &lfd);
    time_t t0 = time(NULL);
    manifest_load_old(g_cfg.home_unix);
    s_rep_trunc = 0;
    if (!g_logf) {
        char logpath[PATH_MAX + 32];
        snprintf(logpath, sizeof(logpath), "%s/tuxgt-launcher.log", g_cfg.home_unix);
        g_logf = fopen(logpath, "a");
    }
    // Data first so ini files and trees exist before any DLL loads.
    stage_includes(&g_cfg, irep, sizeof(irep));
    stage_loaddll(&g_cfg, drep, sizeof(drep));
    stage_unlock(lfd);
    unsigned nok = 0, ntotal = 0;
    unsigned long err = 0;
    void *last_h = NULL;
    load_loaddll(drep, sizeof(drep), &nok, &ntotal, &last_h, &err);
    char msg[PATH_MAX + 256];
    snprintf(msg, sizeof(msg), "load from %s: %u/%u base=%s", via, nok, ntotal, g_cfg.home_unix);
    log_msg(msg);
    int ok = nok == ntotal;
    if (ok && !s_rep_trunc) {
        // Window 2: prune under a fresh lock. Without it the manifest
        // decision could interleave with a sibling's status write; a timed
        // out wait skips prune instead of deciding unlocked.
        if (stage_lock(g_cfg.home_unix, &lfd)) {
            prune_stale(g_cfg.home_unix, irep, drep, t0);
            stage_unlock(lfd);
        } else {
            log_msg("prune skipped, stage lock unavailable");
        }
    } else if (ok) {
        log_msg("status report truncated, skip prune");
    }
    stager = 0;
    __atomic_store_n(&once, 2, __ATOMIC_RELEASE);
    write_status(ok ? "ok" : "failed", last_h, nok, ntotal, err, irep, drep);
}
