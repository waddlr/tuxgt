#define _GNU_SOURCE
#include "tuxgt-launcher.h"
#include <openssl/evp.h>

FILE *g_logf = NULL;

void log_msg(const char *msg) {
    fprintf(stderr, "[tuxgt-launcher] %s\n", msg);
    fflush(stderr);
    // Per-game log, opened at load once the base exists. Helpers without a
    // config section never get here, so they leave no files behind.
    if (g_logf) {
        fprintf(g_logf, "[tuxgt-launcher pid=%d] %s\n", (int)getpid(), msg);
        fflush(g_logf);
    }
}

// Debug-only log: same destination as log_msg (stderr + per-game file),
// gated on the app's live toggle via TUXGT_DEBUG=1 in the launch env.
void log_debug(const char *msg) {
    const char *env = getenv("TUXGT_DEBUG");
    if (env && !strcmp(env, "1")) log_msg(msg);
}

int self_dir(char *out, size_t n) {
    Dl_info info;
    if (!dladdr((void *)self_dir, &info) || !info.dli_fname) return 0;
    snprintf(out, n, "%s", info.dli_fname);
    char *slash = strrchr(out, '/');
    if (slash) *slash = 0;
    return 1;
}

int file_exists(const char *p) {
    struct stat st;
    return stat(p, &st) == 0;
}

int mkdir_p(const char *path) {
    char tmp[PATH_MAX];
    snprintf(tmp, sizeof(tmp), "%s", path);
    if (!tmp[0]) return 0;
    for (char *p = tmp + 1; *p; p++) {
        if (*p != '/') continue;
        *p = 0;
        if (mkdir(tmp, 0755) != 0 && errno != EEXIST) {
            *p = '/';
            return 0;
        }
        *p = '/';
    }
    if (mkdir(tmp, 0755) != 0 && errno != EEXIST) return 0;
    struct stat st;
    return stat(path, &st) == 0 && S_ISDIR(st.st_mode);
}

void exe_stem(const char *path, char *out, size_t n) {
    const char *leaf = path;
    for (const char *p = path; p && *p; p++)
        if (*p == '/' || *p == '\\') leaf = p + 1;
    snprintf(out, n, "%s", leaf);
    size_t len = strlen(out);
    if (len >= 4) {
        char *ext = out + len - 4;
        if (ext[0] == '.' && (ext[1] == 'e' || ext[1] == 'E') && (ext[2] == 'x' || ext[2] == 'X') &&
            (ext[3] == 'e' || ext[3] == 'E'))
            *ext = 0;
    }
    if (!out[0]) snprintf(out, n, "unknown");
}

// Wine puts the target PE in cmdline. First .exe arg is the game/harness.
int wine_cmdline_exe(char *out, size_t n) {
    FILE *f = fopen("/proc/self/cmdline", "r");
    if (!f) return 0;
    static char buf[TUXGT_CMDLINE_MAX];
    size_t nread = fread(buf, 1, sizeof(buf) - 1, f);
    fclose(f);
    if (!nread) return 0;
    buf[nread] = 0;
    for (size_t i = 0; i < nread;) {
        const char *arg = buf + i;
        size_t len = strlen(arg);
        i += len + 1;
        if (len < 5) continue;
        const char *ext = arg + len - 4;
        if (!(ext[0] == '.' && (ext[1] == 'e' || ext[1] == 'E') && (ext[2] == 'x' || ext[2] == 'X') &&
              (ext[3] == 'e' || ext[3] == 'E')))
            continue;
        snprintf(out, n, "%s", arg);
        return 1;
    }
    return 0;
}

int wine_game_process(void) {
    char exe[PATH_MAX];
    ssize_t n = readlink("/proc/self/exe", exe, sizeof(exe) - 1);
    if (n < 0) return 0;
    exe[n] = 0;
    if (strcasestr(exe, "wineserver")) return 0;
    if (strcasestr(exe, "pressure-vessel")) return 0;
    if (strcasestr(exe, "pv-adverb")) return 0;
    if (strcasestr(exe, "bwrap")) return 0;
    if (strcasestr(exe, "python")) return 0;
    if (strcasestr(exe, "reaper")) return 0;
    if (strcasestr(exe, "steam-runtime")) return 0;
    return strcasestr(exe, "wine") != NULL;
}

/* ---- ini ---- */

void trim(char *s) {
    while (*s == ' ' || *s == '\t') memmove(s, s + 1, strlen(s));
    size_t n = strlen(s);
    while (n && (s[n - 1] == ' ' || s[n - 1] == '\t' || s[n - 1] == '\r' || s[n - 1] == '\n')) s[--n] = 0;
}

// Scan core: for each sect/key match call cb(raw value after '=', ctx);
// stop when cb returns nonzero. Returns 1 when the file opened.
static int ini_scan(const char *path, const char *sect, const char *key, int (*cb)(const char *val, void *ctx),
                    void *ctx) {
    FILE *f = fopen(path, "r");
    if (!f) return 0;
    char line[1024], cur[256] = {0};
    while (fgets(line, sizeof(line), f)) {
        char *s = line;
        while (*s == ' ' || *s == '\t') s++;
        if (!*s || *s == ';' || *s == '#' || *s == '\n' || *s == '\r') continue;
        if (*s == '[') {
            char *e = strchr(s, ']');
            if (!e) continue;
            *e = 0;
            snprintf(cur, sizeof(cur), "%s", s + 1);
            trim(cur);
            continue;
        }
        if (strcasecmp(cur, sect) != 0) continue;
        char *eq = strchr(s, '=');
        if (!eq) continue;
        *eq = 0;
        trim(s);
        if (strcasecmp(s, key) != 0) continue;
        if (cb(eq + 1, ctx)) break;
    }
    fclose(f);
    return 1;
}

struct ini_first {
    char *out;
    size_t n;
    int found;
};

static int ini_first_cb(const char *val, void *p) {
    struct ini_first *a = p;
    snprintf(a->out, a->n, "%s", val);
    trim(a->out);
    a->found = 1;
    return 1;
}

// First value of sect/key. Returns 1 if found.
int ini_get(const char *path, const char *sect, const char *key, char *out, size_t n) {
    struct ini_first a = {out, n, 0};
    ini_scan(path, sect, key, ini_first_cb, &a);
    return a.found;
}

struct ini_join {
    char *out;
    size_t n;
    size_t used;
};

static int ini_join_cb(const char *val, void *p) {
    struct ini_join *a = p;
    char v[1024];
    snprintf(v, sizeof(v), "%s", val);
    trim(v);
    if (!v[0]) return 0;
    if (a->used) {
        if (a->used + 2 >= a->n) return 1;
        strcpy(a->out + a->used, ", ");
        a->used += 2;
    }
    size_t vl = strlen(v);
    if (a->used + vl >= a->n) vl = a->n - a->used - 1;
    memcpy(a->out + a->used, v, vl);
    a->used += vl;
    a->out[a->used] = 0;
    return 0;
}

// All occurrences of sect/key joined with ", " (for LoadDLL/IncludeFile lists).
int ini_get_list(const char *path, const char *sect, const char *key, char *out, size_t n) {
    struct ini_join a = {out, n, 0};
    if (!ini_scan(path, sect, key, ini_join_cb, &a)) return 0;
    if (!a.used) out[0] = 0;
    return a.used > 0;
}

int ini_has_sect(const char *path, const char *sect) {
    FILE *f = fopen(path, "r");
    if (!f) return 0;
    char line[256], cur[256] = {0};
    int found = 0;
    while (fgets(line, sizeof(line), f)) {
        char *s = line;
        while (*s == ' ' || *s == '\t') s++;
        if (*s != '[') continue;
        char *e = strchr(s, ']');
        if (!e) continue;
        *e = 0;
        snprintf(cur, sizeof(cur), "%s", s + 1);
        trim(cur);
        if (strcasecmp(cur, sect) == 0) {
            found = 1;
            break;
        }
    }
    fclose(f);
    return found;
}

int find_init_ini(char *out, size_t n) {
    const char *e = getenv("TUXGT_LAUNCHER_INI");
    if (e && e[0] && file_exists(e)) {
        snprintf(out, n, "%s", e);
        return 1;
    }
    char sd[PATH_MAX];
    if (self_dir(sd, sizeof(sd))) {
        snprintf(out, n, "%s/tuxgt-launcher.ini", sd);
        if (file_exists(out)) return 1;
    }
    return 0;
}

// Resolve spec: absolute stays, else relative to the init ini's dir.
void resolve_from_ini(const char *init_path, const char *spec, char *out, size_t n) {
    if (!spec || !spec[0]) {
        out[0] = 0;
        return;
    }
    if (spec[0] == '/') {
        snprintf(out, n, "%s", spec);
        return;
    }
    if (spec[0] == '~' && (spec[1] == 0 || spec[1] == '/')) {
        const char *h = getenv("HOME");
        if (!h || !h[0]) h = "/tmp";
        if (!spec[1])
            snprintf(out, n, "%s", h);
        else
            snprintf(out, n, "%s/%s", h, spec + 2);
        return;
    }
    char dir[PATH_MAX];
    snprintf(dir, sizeof(dir), "%s", init_path);
    char *slash = strrchr(dir, '/');
    if (slash) *slash = 0;
    else snprintf(dir, sizeof(dir), ".");
    snprintf(out, n, "%s/%s", dir, spec);
}

void default_games_dir(char *out, size_t n) {
    char sd[PATH_MAX];
    if (self_dir(sd, sizeof(sd))) {
        char launcher[PATH_MAX];
        snprintf(launcher, sizeof(launcher), "%s/tuxgt-launcher", sd);
        if (file_exists(launcher)) {
            snprintf(out, n, "%s/games", sd);
            return;
        }
    }
    const char *h = getenv("HOME");
    if (!h || !h[0]) h = "/tmp";
    snprintf(out, n, "%s/tuxgt/games", h);
}
long file_size(const char *p) {
    struct stat st;
    if (stat(p, &st) != 0) return -1;
    return (long)st.st_size;
}

/* ---- sha256 via libcrypto (EVP, dynamic NEEDED libcrypto.so.3; see E11) ---- */

int file_sha256(const char *path, unsigned char out[32]) {
    FILE *f = fopen(path, "rb");
    if (!f) return 0;
    EVP_MD_CTX *ctx = EVP_MD_CTX_new();
    if (!ctx) {
        fclose(f);
        return 0;
    }
    static unsigned char buf[TUXGT_IO_BUF];
    size_t n;
    int ok = EVP_DigestInit_ex(ctx, EVP_sha256(), NULL);
    while (ok && (n = fread(buf, 1, sizeof(buf), f)) > 0) ok = EVP_DigestUpdate(ctx, buf, n);
    if (ok && !ferror(f)) {
        unsigned int len = 0;
        ok = EVP_DigestFinal_ex(ctx, out, &len) && len == 32;
    } else {
        ok = 0;
    }
    EVP_MD_CTX_free(ctx);
    fclose(f);
    return ok;
}

int same_digest(const char *a, const char *b) {
    unsigned char da[32], db[32];
    return file_sha256(a, da) && file_sha256(b, db) && memcmp(da, db, 32) == 0;
}

// Copy src -> dst via dst.tmp + rename. Returns 1 on success.
int copy_via_tmp(const char *src, const char *dst) {
    char tmp[PATH_MAX + 24];
    snprintf(tmp, sizeof(tmp), "%s.tmp", dst);
    FILE *in = fopen(src, "rb"), *out = NULL;
    if (in) out = fopen(tmp, "wb");
    int ok = 0;
    if (in && out) {
        static char buf[TUXGT_IO_BUF];
        size_t k;
        ok = 1;
        while ((k = fread(buf, 1, sizeof(buf), in)) > 0)
            if (fwrite(buf, 1, k, out) != k) {
                ok = 0;
                break;
            }
        if (ok && ferror(in)) ok = 0;
        if (ok && (fflush(out) != 0 || fsync(fileno(out)) != 0)) ok = 0;
    }
    if (out) fclose(out);
    if (in) fclose(in);
    if (ok && rename(tmp, dst) == 0) return 1;
    unlink(tmp);
    return 0;
}

// Copy src -> dst, then verify the bytes on disk match src; retry a fresh
// copy on mismatch (the depot read raced a concurrent writer, e.g. app
// Apply mid-launch). Size-checked before hashing so a still-growing depot
// file retries without burning hashes; 75ms between attempts lets a slow
// streaming writer settle. Returns 1 only when dest verifies. At most 3
// attempts; a persistent mismatch reports failure so the run skips prune
// and load instead of handing the game a half-written file.
int copy_verified(const char *src, const char *dst) {
    for (int i = 0; i < 3; i++) {
        if (i) {
            struct timespec ts = {0, 75000000};
            nanosleep(&ts, NULL);
            log_debug("copy retry");
        }
        if (!copy_via_tmp(src, dst)) {
            log_debug("copy failed, retrying");
            continue;
        }
        long ss = file_size(src), ds = file_size(dst);
        if (ss >= 0 && ss == ds && same_digest(src, dst)) return 1;
        log_debug("copy verify mismatch, retrying");
    }
    return 0;
}
