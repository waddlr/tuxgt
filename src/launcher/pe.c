#define _GNU_SOURCE
#include "tuxgt-launcher.h"

int to_utf16(const char *src, uint16_t *dst, size_t dst_n) {
    size_t o = 0;
    const unsigned char *s = (const unsigned char *)src;
    while (*s) {
        uint32_t cp;
        unsigned char c = *s;
        if (c < 0x80) {
            cp = c;
            s++;
        } else if ((c & 0xE0) == 0xC0) {
            if ((s[1] & 0xC0) != 0x80) return 0;
            cp = ((uint32_t)(c & 0x1F) << 6) | (s[1] & 0x3F);
            if (cp < 0x80) return 0;
            s += 2;
        } else if ((c & 0xF0) == 0xE0) {
            if ((s[1] & 0xC0) != 0x80 || (s[2] & 0xC0) != 0x80) return 0;
            cp = ((uint32_t)(c & 0x0F) << 12) | ((uint32_t)(s[1] & 0x3F) << 6) | (s[2] & 0x3F);
            if (cp < 0x800 || (cp >= 0xD800 && cp <= 0xDFFF)) return 0;
            s += 3;
        } else if ((c & 0xF8) == 0xF0) {
            if ((s[1] & 0xC0) != 0x80 || (s[2] & 0xC0) != 0x80 || (s[3] & 0xC0) != 0x80) return 0;
            cp = ((uint32_t)(c & 0x07) << 18) | ((uint32_t)(s[1] & 0x3F) << 12) |
                 ((uint32_t)(s[2] & 0x3F) << 6) | (s[3] & 0x3F);
            if (cp < 0x10000 || cp > 0x10FFFF) return 0;
            s += 4;
        } else {
            return 0;
        }
        if (cp > 0xFFFF) {
            if (o + 2 >= dst_n) return 0;
            uint32_t u = cp - 0x10000;
            dst[o++] = (uint16_t)(0xD800 + (u >> 10));
            dst[o++] = (uint16_t)(0xDC00 + (u & 0x3FF));
        } else {
            if (o + 1 >= dst_n) return 0;
            dst[o++] = (uint16_t)cp;
        }
    }
    if (o >= dst_n) return 0;
    dst[o] = 0;
    return 1;
}

void unix_to_z(const char *unix_path, char *out, size_t n) {
    snprintf(out, n, "Z:%s", unix_path);
    for (char *p = out; *p; p++)
        if (*p == '/') *p = '\\';
}

unsigned long parse_hex_ul(const char *p) {
    unsigned long start = 0;
    for (; *p && *p != '-'; p++) {
        unsigned v = 0;
        if (*p >= '0' && *p <= '9') v = (unsigned)(*p - '0');
        else if (*p >= 'a' && *p <= 'f') v = (unsigned)(*p - 'a' + 10);
        else if (*p >= 'A' && *p <= 'F') v = (unsigned)(*p - 'A' + 10);
        else break;
        start = (start << 4) | v;
    }
    return start;
}

uint8_t *maps_module_base(const char *needle) {
    FILE *f = fopen("/proc/self/maps", "r");
    if (!f) return NULL;
    char line[512];
    uint8_t *mz = NULL, *any = NULL;
    while (fgets(line, sizeof(line), f)) {
        if (!strcasestr(line, needle)) continue;
        unsigned long start = parse_hex_ul(line);
        if (!start) continue;
        uint8_t *p = (uint8_t *)start;
        if (!any) any = p;
        if (p[0] == 'M' && p[1] == 'Z') {
            mz = p;
            break;
        }
    }
    fclose(f);
    return mz ? mz : any;
}

static void *pe_export_n(uint8_t *base, const char *name, int depth) {
    if (!base || base[0] != 'M' || base[1] != 'Z') return NULL;
    uint32_t e_lfanew = *(uint32_t *)(base + 0x3C);
    if (e_lfanew < 64 || e_lfanew > 0x10000) return NULL;
    uint8_t *nt = base + e_lfanew;
    if (nt[0] != 'P' || nt[1] != 'E') return NULL;
    uint8_t *opt = nt + 24;
    uint16_t magic = *(uint16_t *)opt;
    uint32_t export_rva = *(uint32_t *)(opt + (magic == 0x20b ? 112 : 96));
    uint32_t export_size = *(uint32_t *)(opt + (magic == 0x20b ? 116 : 100));
    if (!export_rva) return NULL;
    uint8_t *exp = base + export_rva;
    uint32_t nnames = *(uint32_t *)(exp + 24);
    uint32_t names_rva = *(uint32_t *)(exp + 32);
    uint32_t ords_rva = *(uint32_t *)(exp + 36);
    uint32_t funcs_rva = *(uint32_t *)(exp + 28);
    uint32_t *names = (uint32_t *)(base + names_rva);
    uint16_t *ords = (uint16_t *)(base + ords_rva);
    uint32_t *funcs = (uint32_t *)(base + funcs_rva);
    for (uint32_t i = 0; i < nnames && i < 4096; i++) {
        const char *n = (const char *)(base + names[i]);
        if (strcmp(n, name) != 0) continue;
        uint32_t rva = funcs[ords[i]];
        if (export_size && rva >= export_rva && rva < export_rva + export_size) {
            if (depth >= 4) return NULL;
            const char *fwd = (const char *)(base + rva);
            const char *dot = strchr(fwd, '.');
            if (!dot || dot == fwd || !dot[1]) return NULL;
            char mod[256], expn[256], needle[280];
            size_t ml = (size_t)(dot - fwd);
            if (ml >= sizeof(mod)) return NULL;
            memcpy(mod, fwd, ml);
            mod[ml] = 0;
            // Named forwards only. MODULE.#ordinal is unused on Wine
            // kernel32/kernelbase (PLAN); treating "#N" as a name returns NULL.
            snprintf(expn, sizeof(expn), "%s", dot + 1);
            if (strchr(mod, '.'))
                snprintf(needle, sizeof(needle), "%s", mod);
            else
                snprintf(needle, sizeof(needle), "%s.dll", mod);
            uint8_t *mb = maps_module_base(needle);
            if (!mb) return NULL;
            return pe_export_n(mb, expn, depth + 1);
        }
        return base + rva;
    }
    return NULL;
}

void *pe_export(uint8_t *base, const char *name) {
    return pe_export_n(base, name, 0);
}
