/* Internal decls for libtuxgt-launcher.so. Not a public API. */
#ifndef TUXGT_LAUNCHER_H
#define TUXGT_LAUNCHER_H

#define _GNU_SOURCE
#include <dirent.h>
#include <dlfcn.h>
#include <errno.h>
#include <limits.h>
#include <poll.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

/* ---- budgets: one documented home for loader size caps (values frozen) ----
 * Truncation is silent by design — snprintf/vl-cap/break drop the excess,
 * status reports set s_rep_trunc, manifest_add drops the extras. Documented
 * here so the caps stay in sync; do not "fix" the silence piecemeal.
 * TUXGT_LIST_BUDGET must equal Rust INI_LIST_BUDGET (= 8192, prewire/mod.rs):
 * joined LoadDLL/IncludeFile lists are built against it on both sides. */
#define TUXGT_LIST_BUDGET 8192 /* LoadDLL/IncludeFile lists, tokenize copies, rep bufs */
#define TUXGT_CMDLINE_MAX 4096 /* /proc/self/cmdline scan for the target .exe */
#define TUXGT_IO_BUF 65536 /* sha256/copy streaming chunk (chunk size only, no truncation) */
#define TUXGT_STATUS_MAX 32768 /* status-ini read for manifest_load_old */
#define TUXGT_PAGE 4096 /* gate mmap/mprotect granularity; loader assumes 4K pages */
#define MANIFEST_MAX 320 /* max recorded staged dests */
#define MANIFEST_NAMELEN 512 /* max recorded dest length */

struct game_cfg {
    int ok; /* exe stem has a section in the init ini */
    char type[64]; /* informational only */
    char home_unix[PATH_MAX]; /* GamesDir/<stem>/ */
    char depot_unix[PATH_MAX]; /* central depot (may be empty) */
    char loaddll[TUXGT_LIST_BUDGET]; /* LoadDLL: staged, then LoadLibrary in order */
    char includes[TUXGT_LIST_BUDGET]; /* IncludeFile: staged only */
};

typedef void *__attribute__((ms_abi)) (*loadlibraryw_fn)(const uint16_t *);

extern int (*real_poll)(struct pollfd *, nfds_t, int);
extern int (*real_ppoll)(struct pollfd *, nfds_t, const struct timespec *, const sigset_t *);
extern FILE *g_logf;
extern struct game_cfg g_cfg;
extern int g_cfg_done;
extern int s_rep_trunc;
extern int g_in_ll;
extern int g_armed;
extern int g_kind;
extern int g_busy;
extern loadlibraryw_fn g_LoadLibraryW;
extern loadlibraryw_fn g_orig_llw;
extern void *__attribute__((ms_abi)) (*g_orig_llew)(const uint16_t *, void *, uint32_t);
extern unsigned long __attribute__((ms_abi)) (*g_orig_gtc)(void);
extern unsigned long long __attribute__((ms_abi)) (*g_orig_gtc64)(void);
extern int __attribute__((ms_abi)) (*g_orig_qpc)(void *);
extern void __attribute__((ms_abi)) (*g_orig_sleep)(unsigned long);
extern unsigned long __attribute__((ms_abi)) (*g_orig_sleepex)(unsigned long, int);
extern unsigned long __attribute__((ms_abi)) (*g_GetLastError)(void);
extern int __attribute__((ms_abi)) (*g_SetEnvW)(const uint16_t *, const uint16_t *);
extern int __attribute__((ms_abi)) (*g_FlushIC)(void *proc, const void *addr, size_t n);
extern unsigned long __attribute__((ms_abi)) (*g_GetModuleFileNameW)(void *mod, uint16_t *buf, unsigned long n);
extern uint8_t *g_gated_fn[16];
extern int g_gated_n;

void log_msg(const char *msg);
void log_debug(const char *msg);
int self_dir(char *out, size_t n);
int file_exists(const char *p);
int mkdir_p(const char *path);
void exe_stem(const char *path, char *out, size_t n);
int wine_cmdline_exe(char *out, size_t n);
int wine_game_process(void);
void trim(char *s);
int ini_get(const char *path, const char *sect, const char *key, char *out, size_t n);
int ini_get_list(const char *path, const char *sect, const char *key, char *out, size_t n);
int ini_has_sect(const char *path, const char *sect);
int find_init_ini(char *out, size_t n);
void resolve_from_ini(const char *init_path, const char *spec, char *out, size_t n);
void default_games_dir(char *out, size_t n);
void resolve_cfg(const char *stem, struct game_cfg *c);
long file_size(const char *p);
int file_sha256(const char *path, unsigned char out[32]);
int same_digest(const char *a, const char *b);
int copy_via_tmp(const char *src, const char *dst);
int copy_verified(const char *src, const char *dst);
int split_mapping(const char *tok, char *dest, size_t dn, char *src, size_t sn, int *is_tree);
void stage_loaddll(const struct game_cfg *c, char *rep, size_t n);
void stage_includes(const struct game_cfg *c, char *rep, size_t n);
void manifest_load_old(const char *home);
void prune_stale(const char *home, const char *irep, const char *drep, time_t since);
int to_utf16(const char *src, uint16_t *dst, size_t dst_n);
void unix_to_z(const char *unix_path, char *out, size_t n);
unsigned long parse_hex_ul(const char *p);
uint8_t *maps_module_base(const char *needle);
void *pe_export(uint8_t *base, const char *name);
int is_reshade_name(const char *dest);
void reshade_quirk(void);
void load_loaddll(char *rep, size_t n, unsigned *nok, unsigned *ntotal, void **last_h,
                  unsigned long *err);
void write_status(const char *result, void *handle, unsigned lok, unsigned ltotal, unsigned long err,
                  const char *irep, const char *drep);
void load_once(const char *via);
void load_wait(void);
void *__attribute__((ms_abi)) gate_llw(const uint16_t *n);
void *__attribute__((ms_abi)) gate_llew(const uint16_t *n, void *file, uint32_t flags);
unsigned long __attribute__((ms_abi)) gate_gtc(void);
unsigned long long __attribute__((ms_abi)) gate_gtc64(void);
int __attribute__((ms_abi)) gate_qpc(void *li);
void __attribute__((ms_abi)) gate_sleep(unsigned long ms);
unsigned long __attribute__((ms_abi)) gate_sleepex(unsigned long ms, int alertable);
int install_gate(uint8_t *k32, const char *name, void *elf_gate, void **orig_out);
int arm_pe_inject(void);
void maybe_inject(void);

#endif
