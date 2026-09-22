// Re-entrancy harness for load_loaddll's list walk (see test_load_reentry.sh).
// The loop body calls LoadLibraryW, which may use strtok internally and
// clobber a plain-strtok walk after the first entry. The fake loader below
// dirties the strtok state on every call; both entries must still load.
#define _GNU_SOURCE
#include <stdio.h>
#include <string.h>
#include <strings.h>
#include <limits.h>
#include "tuxgt-launcher.h"

static void *__attribute__((ms_abi)) fake_llw(const uint16_t *n) {
    (void)n;
    char tmp[] = "wine,internal";
    strtok(tmp, ",");
    strtok(NULL, ",");
    return (void *)0x1234;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s <home> <depot>\n", argv[0]);
        return 2;
    }
    memset(&g_cfg, 0, sizeof(g_cfg));
    snprintf(g_cfg.home_unix, sizeof(g_cfg.home_unix), "%s", argv[1]);
    snprintf(g_cfg.depot_unix, sizeof(g_cfg.depot_unix), "%s", argv[2]);
    snprintf(g_cfg.loaddll, sizeof(g_cfg.loaddll), "%s", "a.dll=a.dll, b.dll=b.dll");
    g_orig_llw = fake_llw;
    void *last_h = NULL;
    unsigned nok = 0, ntotal = 0;
    unsigned long err = 0;
    char rep[1024] = {0};
    load_loaddll(rep, sizeof(rep), &nok, &ntotal, &last_h, &err);
    if (nok != 2 || ntotal != 2) {
        fprintf(stderr, "FAIL: want 2/2 got %u/%u rep=[%s]\n", nok, ntotal, rep);
        return 1;
    }
    printf("PASS: load_loaddll survived strtok-dirtying loader (%u/%u)\n", nok, ntotal);
    return 0;
}
