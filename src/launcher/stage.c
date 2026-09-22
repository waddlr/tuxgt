#define _GNU_SOURCE
#include "tuxgt-launcher.h"

// Reject empty, absolute, backslash, and parent-dir escape: staged dests
// stay under the depot/home root. Shared core with Rust `stage::check_rel`;
// two deliberate deltas: `:` is rejected here (loader inputs never
// legitimately contain it — `pfx:` dests are install-adapter-only, never
// preload) and `..` is rejected as any substring, not per component
// (stricter; the loader is the last line of defense).
static int rel_path_ok(const char *p) {
    if (!p || !p[0]) return 0;
    if (p[0] == '/' || p[0] == '\\') return 0;
    if (strchr(p, ':') || strchr(p, '\\') || strstr(p, "..")) return 0;
    return 1;
}

// Split "dest=src" on the first '='; bare "tok" means dest=src=tok.
// Tree form needs both sides ending in '/' (e.g. OptiScaler/=ver/OptiScaler/).
int split_mapping(const char *tok, char *dest, size_t dn, char *src, size_t sn, int *is_tree) {
    const char *eq = strchr(tok, '=');
    if (eq) {
        size_t dl = (size_t)(eq - tok), sl = strlen(eq + 1);
        if (!dl || !sl || dl >= dn || sl >= sn) return 0;
        memcpy(dest, tok, dl);
        dest[dl] = 0;
        snprintf(src, sn, "%s", eq + 1);
    } else {
        if (strlen(tok) >= dn || strlen(tok) >= sn) return 0;
        snprintf(dest, dn, "%s", tok);
        snprintf(src, sn, "%s", tok);
    }
    if (!rel_path_ok(dest) || !rel_path_ok(src)) return 0;
    size_t dl = strlen(dest), sl = strlen(src);
    int dt = dl > 1 && dest[dl - 1] == '/';
    int st = sl > 1 && src[sl - 1] == '/';
    if (dt != st) return 0;
    *is_tree = dt;
    return 1;
}

static int mkdir_parent(const char *path) {
    char tmp[PATH_MAX + 16];
    snprintf(tmp, sizeof(tmp), "%s", path);
    char *slash = strrchr(tmp, '/');
    if (!slash) return 1;
    *slash = 0;
    return mkdir_p(tmp);
}

// Set when a rep buffer fills: the status report is then partial and must
// not be used as a prune manifest.
int s_rep_trunc;

// Append one "dest=result" line; set s_rep_trunc when the buffer is full.
static void report_result(char *rep, size_t n, const char *dest, const char *res) {
    size_t used = strlen(rep);
    if (used + strlen(dest) + strlen(res) + 3 < n) {
        snprintf(rep + used, n - used, "%s=%s\n", dest, res);
    } else {
        s_rep_trunc = 1;
    }
}

// Copy depot/src -> home/dest when missing or content differs (size fast
// path, sha256 when sizes match). Appends "dest=result" report lines.
static void stage_mapped(const char *depot, const char *home, const char *src, const char *dest,
                         char *rep, size_t n, const char *kind) {
    char sabs[PATH_MAX], dabs[PATH_MAX + 16];
    if (snprintf(sabs, sizeof(sabs), "%s/%s", depot, src) >= (int)sizeof(sabs) ||
        snprintf(dabs, sizeof(dabs), "%s/%s", home, dest) >= (int)sizeof(dabs)) {
        log_msg("stage path too long, skipped");
        size_t used = strlen(rep);
        if (used + strlen(dest) + 8 < n) {
            snprintf(rep + used, n - used, "%s=failed\n", dest);
        } else {
            s_rep_trunc = 1;
        }
        return;
    }
    long ss = file_size(sabs), ds = file_size(dabs);
    const char *res = "up-to-date";
    if (ss < 0) {
        res = "missing";
    } else if (ss != ds || !same_digest(sabs, dabs)) {
        res = "failed";
        if (mkdir_parent(dabs) && copy_verified(sabs, dabs)) res = "staged";
        char msg[512];
        snprintf(msg, sizeof(msg), "%s %s %s (%ld bytes)", kind, dest, res, ss);
        log_msg(msg);
    } else {
        char msg[512];
        snprintf(msg, sizeof(msg), "%s %s up-to-date, skipped", kind, dest);
        log_debug(msg);
    }
    report_result(rep, n, dest, res);
}

// Mirror depot tree sdir -> home tree ddir (absolute, no trailing slash):
// mkdir, recurse dirs, stage files depot-wins. Skips non-regular files.
// Reports per-file "reldest=result" lines (reldest rooted at rep_base, e.g.
// reshade-shaders/Shaders/Example.fx) so later runs can tell staged files
// from foreign ones.
static void stage_tree(const char *sdir, const char *ddir, const char *rep_base, char *rep, size_t n,
                       unsigned *staged, unsigned *total) {
    if (!mkdir_p(ddir)) {
        char msg[PATH_MAX + 64];
        snprintf(msg, sizeof(msg), "tree mkdir failed: %s", ddir);
        log_msg(msg);
        return;
    }
    DIR *d = opendir(sdir);
    if (!d) {
        char msg[PATH_MAX + 64];
        snprintf(msg, sizeof(msg), "tree missing: %s", sdir);
        log_msg(msg);
        return;
    }
    struct dirent *e;
    while ((e = readdir(d)) != NULL) {
        if (!strcmp(e->d_name, ".") || !strcmp(e->d_name, "..")) continue;
        if (strchr(e->d_name, '\\') || strstr(e->d_name, "..")) continue;
        char sp[PATH_MAX], dp[PATH_MAX + 16];
        if (snprintf(sp, sizeof(sp), "%s/%s", sdir, e->d_name) >= (int)sizeof(sp) ||
            snprintf(dp, sizeof(dp), "%s/%s", ddir, e->d_name) >= (int)sizeof(dp)) {
            log_msg("tree path too long, skipped");
            continue;
        }
        struct stat st;
        if (lstat(sp, &st) != 0) continue;
        char rel[PATH_MAX];
        snprintf(rel, sizeof(rel), "%s/%s", rep_base, e->d_name);
        if (S_ISDIR(st.st_mode)) {
            stage_tree(sp, dp, rel, rep, n, staged, total);
        } else if (S_ISREG(st.st_mode)) {
            (*total)++;
            const char *res = "up-to-date";
            if ((long)st.st_size != file_size(dp) || !same_digest(sp, dp)) {
                res = "failed";
                if (copy_verified(sp, dp)) {
                    res = "staged";
                    (*staged)++;
                }
            }
            report_result(rep, n, rel, res);
        }
    }
    closedir(d);
}

// Staged lists: `LoadDLL` holds loadable DLLs only (`name`, `dest=src`
// renames/subpaths, e.g. proxy slot `dxgi.dll=optiscaler-v3/OptiScaler.dll`);
// `IncludeFile` holds stage-only data (`dest=src` files plus `destdir/=srcdir/`
// recursive tree mirrors, e.g. `OptiScaler/=optiscaler-v3/OptiScaler/`).
// File entries LoadLibrary in list order; trees stage only. Depot always wins.
static void stage_lists(const char *depot, const char *home, const char *list, int allow_trees, char *rep,
                        size_t n, const char *kind) {
    rep[0] = 0;
    if (!list[0] || !depot[0]) return;
    char work[TUXGT_LIST_BUDGET];
    snprintf(work, sizeof(work), "%s", list);
    for (char *tok = strtok(work, ","); tok; tok = strtok(NULL, ",")) {
        while (*tok == ' ' || *tok == '\t') tok++;
        char *end = tok + strlen(tok);
        while (end > tok && (end[-1] == ' ' || end[-1] == '\t')) *--end = 0;
        char dest[PATH_MAX], src[PATH_MAX];
        int is_tree = 0;
        if (!split_mapping(tok, dest, sizeof(dest), src, sizeof(src), &is_tree)) continue;
        if (is_tree && !allow_trees) continue;
        if (!is_tree) {
            stage_mapped(depot, home, src, dest, rep, n, kind);
            continue;
        }
        dest[strlen(dest) - 1] = 0;
        src[strlen(src) - 1] = 0;
        char sabs[PATH_MAX], dabs[PATH_MAX + 16];
        snprintf(sabs, sizeof(sabs), "%s/%s", depot, src);
        snprintf(dabs, sizeof(dabs), "%s/%s", home, dest);
        unsigned staged = 0, total = 0;
        stage_tree(sabs, dabs, dest, rep, n, &staged, &total);
        char msg[512], sum[64];
        snprintf(sum, sizeof(sum), "tree %u/%u", staged, total);
        snprintf(msg, sizeof(msg), "%s %s/ %s (from %s/)", kind, dest, sum, src);
        log_msg(msg);
    }
}

// LoadDLL payloads stage from the depot. Sidecar DLLs (NGX
// snippets) and proxy slots ride the depot with hash updates; only *.dll
// entries LoadLibrary, in list order.
void stage_loaddll(const struct game_cfg *c, char *rep, size_t n) {
    stage_lists(c->depot_unix, c->home_unix, c->loaddll, 0, rep, n, "dll");
}

// IncludeFile entries stage from the depot but are never loaded:
// companion ini files and directory trees living next to the DLLs.
void stage_includes(const struct game_cfg *c, char *rep, size_t n) {
    stage_lists(c->depot_unix, c->home_unix, c->includes, 1, rep, n, "file");
}

static char s_old_dests[MANIFEST_MAX][MANIFEST_NAMELEN];
static int s_old_n;
static char s_cur_dests[MANIFEST_MAX][MANIFEST_NAMELEN];
static int s_cur_n;

static void manifest_add(char arr[MANIFEST_MAX][MANIFEST_NAMELEN], int *n, const char *dest) {
    if (*n >= MANIFEST_MAX) return;
    size_t l = strlen(dest);
    if (!l || l >= MANIFEST_NAMELEN) return;
    for (int i = 0; i < *n; i++)
        if (!strcmp(arr[i], dest)) return;
    snprintf(arr[*n], MANIFEST_NAMELEN, "%s", dest);
    (*n)++;
}

// Collect dests with staged/up-to-date results from [IncludeFile]/[LoadDLL]
// sections. Handle lines (`dest=0x...`) are not staging evidence and legacy
// `dir/=tree` summaries cannot name files, so both are skipped. Rep buffers
// carry no section headers, so hdrless=1 accepts every line.
static void manifest_parse(const char *text, char arr[MANIFEST_MAX][MANIFEST_NAMELEN], int *n, int hdrless) {
    if (!text) return;
    int want = hdrless;
    const char *p = text;
    char line[1024];
    while (*p) {
        size_t i = 0;
        while (p[i] && p[i] != '\n' && i + 1 < sizeof(line)) {
            line[i] = p[i];
            i++;
        }
        line[i] = 0;
        p += i;
        while (*p && *p != '\n') p++;
        if (*p == '\n') p++;
        while (i && (line[i - 1] == '\r' || line[i - 1] == ' ' || line[i - 1] == '\t')) line[--i] = 0;
        char *s = line;
        while (*s == ' ' || *s == '\t') s++;
        if (!*s) continue;
        if (*s == '[') {
            want = !strcmp(s, "[IncludeFile]") || !strcmp(s, "[LoadDLL]");
            continue;
        }
        if (!want) continue;
        char *eq = strchr(s, '=');
        if (!eq || eq == s) continue;
        *eq = 0;
        if (strcmp(eq + 1, "staged") && strcmp(eq + 1, "up-to-date")) continue;
        size_t dl = strlen(s);
        if (!dl || s[dl - 1] == '/') continue;
        if (!rel_path_ok(s)) continue;
        manifest_add(arr, n, s);
    }
}

// Snapshot the previous run's staged files (our status file doubles as the
// manifest). Absent on first run: nothing recorded, nothing pruned.
void manifest_load_old(const char *home) {
    s_old_n = 0;
    char path[PATH_MAX + 32];
    snprintf(path, sizeof(path), "%s/tuxgt-launcher-status.ini", home);
    FILE *f = fopen(path, "r");
    if (!f) return;
    static char text[TUXGT_STATUS_MAX];
    size_t n = fread(text, 1, sizeof(text) - 1, f);
    fclose(f);
    text[n] = 0;
    manifest_parse(text, s_old_dests, &s_old_n, 0);
}

// Remove files the previous run staged but the current config no longer
// references. Files created after this run started are kept: they belong to
// a concurrent live process whose ini state diverged from ours. Only exact
// recorded paths; foreign and hand-placed files never match. Emptied tree
// roots are rmdir'd, which fails when non-empty so user files inside survive.
// Callers pass the fresh rep buffers as the current manifest and run this on
// ok results only.
void prune_stale(const char *home, const char *irep, const char *drep, time_t since) {
    s_cur_n = 0;
    manifest_parse(irep, s_cur_dests, &s_cur_n, 1);
    manifest_parse(drep, s_cur_dests, &s_cur_n, 1);
    unsigned pruned = 0;
    char parents[MANIFEST_MAX][MANIFEST_NAMELEN];
    int npar = 0;
    char full[PATH_MAX + MANIFEST_NAMELEN + 8];
    for (int i = 0; i < s_old_n; i++) {
        int keep = 0;
        for (int j = 0; j < s_cur_n; j++)
            if (!strcmp(s_old_dests[i], s_cur_dests[j])) {
                keep = 1;
                break;
            }
        if (keep) continue;
        snprintf(full, sizeof(full), "%s/%s", home, s_old_dests[i]);
        // Fresh since this run started: a concurrent live process staged it
        // after we began (ini states diverged mid-launch). Never delete
        // another process's in-flight files; stale ones age out next run.
        // Clamped to 60s of future skew so a forward clock step cannot
        // protect a genuinely stale file forever.
        struct stat fst;
        time_t now = time(NULL);
        if (stat(full, &fst) == 0 && fst.st_mtime >= since && fst.st_mtime <= now + 60) {
            char msg[MANIFEST_NAMELEN + 48];
            snprintf(msg, sizeof(msg), "prune skipped fresh file %s", s_old_dests[i]);
            log_msg(msg);
            continue;
        }
        if (unlink(full) == 0) {
            pruned++;
            char msg[MANIFEST_NAMELEN + 32];
            snprintf(msg, sizeof(msg), "pruned %s", s_old_dests[i]);
            log_msg(msg);
            // Record every ancestor dir for rmdir attempts (deepest first
            // later); rmdir fails when non-empty, so user files survive.
            const char *slash = strrchr(s_old_dests[i], '/');
            while (slash && slash != s_old_dests[i]) {
                char par[MANIFEST_NAMELEN];
                size_t pl = (size_t)(slash - s_old_dests[i]);
                if (!pl || pl >= sizeof(par)) break;
                memcpy(par, s_old_dests[i], pl);
                par[pl] = 0;
                manifest_add(parents, &npar, par);
                slash = (char *)memrchr(s_old_dests[i], '/', pl);
            }
        }
    }
    for (int i = 0; i < npar; i++)
        for (int j = i + 1; j < npar; j++)
            if (strlen(parents[i]) < strlen(parents[j])) {
                char t[MANIFEST_NAMELEN];
                snprintf(t, sizeof(t), "%s", parents[i]);
                snprintf(parents[i], sizeof(parents[i]), "%s", parents[j]);
                snprintf(parents[j], sizeof(parents[j]), "%s", t);
            }
    for (int i = 0; i < npar; i++) {
        snprintf(full, sizeof(full), "%s/%s", home, parents[i]);
        rmdir(full); // succeeds only when empty
    }
    if (pruned) {
        char msg[64];
        snprintf(msg, sizeof(msg), "pruned %u unreferenced file(s)", pruned);
        log_msg(msg);
    }
}

