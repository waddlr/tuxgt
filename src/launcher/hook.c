#define _GNU_SOURCE
// libtuxgt-launcher.so: LD_PRELOAD that stages injector payloads from a
// central depot into GamesDir/<exe>/ and LoadLibrary's them, without ever
// writing the game dir. Per-game opt-in: only processes whose exe stem has
// a section in the init ini do anything. Config is injector-agnostic:
// LoadDLL entries (staged, then loaded in order) plus IncludeFile entries
// (staged only). No DLL names or injector kinds are hard-coded; the single
// ReShade-specific bit is a dest-basename quirk (stock ReShade stems need
// <base>/ReShade.ini to exist and honor RESHADE_BASE_PATH_OVERRIDE).
#include "tuxgt-launcher.h"

int (*real_poll)(struct pollfd *, nfds_t, int);
int (*real_ppoll)(struct pollfd *, nfds_t, const struct timespec *, const sigset_t *);

int g_armed;

/* 0 unknown, 1 skip forever, 2 wine (arm gates when kernel32 ready) */
int g_kind;
int g_busy;

int arm_pe_inject(void) {
    uint8_t *k32 = maps_module_base("kernel32.dll");
    if (!k32) return 0;
    g_LoadLibraryW = (loadlibraryw_fn)pe_export(k32, "LoadLibraryW");
    if (!g_LoadLibraryW) return 0;
    g_GetLastError = (unsigned long __attribute__((ms_abi)) (*)(void))pe_export(k32, "GetLastError");
    g_FlushIC = (int __attribute__((ms_abi)) (*)(void *, const void *, size_t))pe_export(
        k32, "FlushInstructionCache");
    g_GetModuleFileNameW = (unsigned long __attribute__((ms_abi)) (*)(void *, uint16_t *, unsigned long))pe_export(
        k32, "GetModuleFileNameW");
    uint8_t *kbase = maps_module_base("kernelbase.dll");
    uint8_t *target = kbase ? kbase : k32;
    g_SetEnvW = (int __attribute__((ms_abi)) (*)(const uint16_t *, const uint16_t *))pe_export(
        target, "SetEnvironmentVariableW");
    if (!g_SetEnvW && target != k32)
        g_SetEnvW = (int __attribute__((ms_abi)) (*)(const uint16_t *, const uint16_t *))pe_export(
            k32, "SetEnvironmentVariableW");
    int n = 0;
    n += install_gate(k32, "LoadLibraryW", (void *)gate_llw, (void **)&g_orig_llw);
    if (!g_orig_llw) n += install_gate(target, "LoadLibraryW", (void *)gate_llw, (void **)&g_orig_llw);
    n += install_gate(target, "LoadLibraryExW", (void *)gate_llew, (void **)&g_orig_llew);
    if (!g_orig_llew) n += install_gate(k32, "LoadLibraryExW", (void *)gate_llew, (void **)&g_orig_llew);
    if (g_orig_llw) g_LoadLibraryW = g_orig_llw;
    n += install_gate(target, "Sleep", (void *)gate_sleep, (void **)&g_orig_sleep);
    if (!g_orig_sleep) n += install_gate(k32, "Sleep", (void *)gate_sleep, (void **)&g_orig_sleep);
    n += install_gate(target, "GetTickCount64", (void *)gate_gtc64, (void **)&g_orig_gtc64);
    if (!g_orig_gtc64) n += install_gate(k32, "GetTickCount64", (void *)gate_gtc64, (void **)&g_orig_gtc64);
    n += install_gate(target, "GetTickCount", (void *)gate_gtc, (void **)&g_orig_gtc);
    if (!g_orig_gtc) n += install_gate(k32, "GetTickCount", (void *)gate_gtc, (void **)&g_orig_gtc);
    n += install_gate(target, "QueryPerformanceCounter", (void *)gate_qpc, (void **)&g_orig_qpc);
    if (!g_orig_qpc) n += install_gate(k32, "QueryPerformanceCounter", (void *)gate_qpc, (void **)&g_orig_qpc);
    n += install_gate(target, "SleepEx", (void *)gate_sleepex, (void **)&g_orig_sleepex);
    if (!g_orig_sleepex) n += install_gate(k32, "SleepEx", (void *)gate_sleepex, (void **)&g_orig_sleepex);
    {
        char msg[128];
        snprintf(msg, sizeof(msg), "armed gates=%d llw=%d llew=%d sleep=%d gtc=%d gtc64=%d qpc=%d", n,
                 g_orig_llw ? 1 : 0, g_orig_llew ? 1 : 0, g_orig_sleep ? 1 : 0, g_orig_gtc ? 1 : 0,
                 g_orig_gtc64 ? 1 : 0, g_orig_qpc ? 1 : 0);
        log_msg(msg);
    }
    // Resolve config now (Unix side, safe): opt-in per exe stem.
    char cmd[PATH_MAX] = {0};
    if (wine_cmdline_exe(cmd, sizeof(cmd))) {
        char stem[256];
        exe_stem(cmd, stem, sizeof(stem));
        resolve_cfg(stem, &g_cfg);
    }
    g_cfg_done = 1;
    g_armed = n > 0;
    return n > 0;
}

void maybe_inject(void) {
    if (g_kind == 1 || g_busy) return;
    g_busy = 1;
    if (g_kind == 0) {
        g_kind = wine_game_process() ? 2 : 1;
        g_busy = 0;
        return;
    }
    if (g_kind == 2 && !g_armed) {
        if (maps_module_base("kernel32.dll")) arm_pe_inject();
    }
    g_busy = 0;
}

__attribute__((constructor)) static void init_reals(void) {
    real_poll = (int (*)(struct pollfd *, nfds_t, int))dlsym(RTLD_NEXT, "poll");
    real_ppoll = (int (*)(struct pollfd *, nfds_t, const struct timespec *, const sigset_t *))dlsym(RTLD_NEXT, "ppoll");
}

__attribute__((visibility("default")))
int poll(struct pollfd *fds, nfds_t nfds, int timeout) {
    if (!real_poll) real_poll = (int (*)(struct pollfd *, nfds_t, int))dlsym(RTLD_NEXT, "poll");
    maybe_inject();
    return real_poll(fds, nfds, timeout);
}

__attribute__((visibility("default")))
int ppoll(struct pollfd *fds, nfds_t nfds, const struct timespec *ts, const sigset_t *sig) {
    if (!real_ppoll)
        real_ppoll = (int (*)(struct pollfd *, nfds_t, const struct timespec *, const sigset_t *))dlsym(RTLD_NEXT, "ppoll");
    maybe_inject();
    return real_ppoll(fds, nfds, ts, sig);
}
