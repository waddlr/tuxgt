#define _GNU_SOURCE
#include "tuxgt-launcher.h"

void *__attribute__((ms_abi)) gate_llw(const uint16_t *n) {
    load_wait();
    if (g_in_ll) return g_orig_llw(n);
    load_once("LoadLibraryW");
    return g_orig_llw(n);
}

void *__attribute__((ms_abi)) gate_llew(const uint16_t *n, void *file, uint32_t flags) {
    load_wait();
    if (g_in_ll) return g_orig_llew(n, file, flags);
    load_once("LoadLibraryExW");
    return g_orig_llew(n, file, flags);
}

unsigned long __attribute__((ms_abi)) gate_gtc(void) {
    load_once("GetTickCount");
    return g_orig_gtc();
}
unsigned long long __attribute__((ms_abi)) gate_gtc64(void) {
    load_once("GetTickCount64");
    return g_orig_gtc64();
}
int __attribute__((ms_abi)) gate_qpc(void *li) {
    load_once("QueryPerformanceCounter");
    return g_orig_qpc(li);
}
void __attribute__((ms_abi)) gate_sleep(unsigned long ms) {
    load_once("Sleep");
    g_orig_sleep(ms);
}
unsigned long __attribute__((ms_abi)) gate_sleepex(unsigned long ms, int alertable) {
    load_once("SleepEx");
    return g_orig_sleepex(ms, alertable);
}

static int modrm_size(const uint8_t *p) {
    unsigned char modrm = p[0];
    int mod = modrm >> 6;
    int rm = modrm & 7;
    int n = 1;
    if (mod != 3 && rm == 4) {
        n++;
        unsigned char sib = p[1];
        if ((sib & 7) == 5 && mod == 0) n += 4;
    }
    if (mod == 1) n += 1;
    else if (mod == 2) n += 4;
    else if (mod == 0 && rm == 5) n += 4;
    return n;
}

static int insn_len(const uint8_t *p) {
    const uint8_t *s = p;
    for (int i = 0; i < 4; i++) {
        unsigned char b = *p;
        if (b == 0x66 || b == 0x67 || b == 0xF0 || b == 0xF2 || b == 0xF3) {
            p++;
            continue;
        }
        if (b >= 0x40 && b <= 0x4F) {
            p++;
            break;
        }
        break;
    }
    unsigned char op = *p++;
    if (op == 0x0F) {
        unsigned char op2 = *p++;
        if (op2 == 0x1F || op2 == 0x0D) return (int)(p - s) + modrm_size(p);
        if (op2 >= 0x80 && op2 <= 0x8F) return (int)(p - s) + 4;
        return (int)(p - s) + modrm_size(p);
    }
    if ((op & 0xF0) == 0x50) return (int)(p - s);
    if (op == 0x90 || op == 0x9C || op == 0x9D || op == 0xCC || op == 0xC3) return (int)(p - s);
    if (op == 0x6A) return (int)(p - s) + 1;
    if (op == 0x68) return (int)(p - s) + 4;
    if (op == 0xE8 || op == 0xE9) return (int)(p - s) + 4;
    if (op == 0xEB) return (int)(p - s) + 1;
    if (op == 0xC2) return (int)(p - s) + 2;
    if (op == 0x83) return (int)(p - s) + modrm_size(p) + 1;
    if (op == 0x81) return (int)(p - s) + modrm_size(p) + 4;
    if (op == 0xC6) return (int)(p - s) + modrm_size(p) + 1;
    if (op == 0xC7) return (int)(p - s) + modrm_size(p) + 4;
    if (op == 0x84 || op == 0x85 || op == 0x88 || op == 0x89 || op == 0x8A || op == 0x8B || op == 0x8D ||
        op == 0xFF) {
        return (int)(p - s) + modrm_size(p);
    }
    if (op >= 0x70 && op <= 0x7F) return (int)(p - s) + 1;
    if (op >= 0xB8 && op <= 0xBF) {
        int rex_w = (s[0] >= 0x48 && s[0] <= 0x4F && (s[0] & 8));
        return (int)(p - s) + (rex_w ? 8 : 4);
    }
    if (op == 0x31 || op == 0x33) return (int)(p - s) + modrm_size(p);
    return -1;
}

static int stolen_len(const uint8_t *fn) {
    int stolen = 0;
    while (stolen < 5) {
        int n = insn_len(fn + stolen);
        if (n <= 0 || n > 15) return -1;
        stolen += n;
        if (stolen > 15) return -1;
    }
    return stolen;
}

// Relative call/jmp/jcc or RIP-relative ModRM cannot be copied into the stub.
static int insn_is_rel(const uint8_t *p, int n) {
    int i = 0;
    while (i < n && (p[i] == 0x66 || p[i] == 0x67 || p[i] == 0xF0 || p[i] == 0xF2 || p[i] == 0xF3 ||
                     (p[i] >= 0x40 && p[i] <= 0x4F)))
        i++;
    if (i >= n) return 0;
    unsigned char op = p[i++];
    if (op == 0xE8 || op == 0xE9 || op == 0xEB || (op >= 0x70 && op <= 0x7F)) return 1;
    if (op == 0x0F) {
        if (i >= n) return 0;
        unsigned char op2 = p[i++];
        if (op2 >= 0x80 && op2 <= 0x8F) return 1;
        if (i >= n) return 0;
        unsigned char modrm = p[i];
        return (modrm >> 6) == 0 && (modrm & 7) == 5;
    }
    if (op == 0x83 || op == 0x81 || op == 0xC6 || op == 0xC7 || op == 0x84 || op == 0x85 || op == 0x88 ||
        op == 0x89 || op == 0x8A || op == 0x8B || op == 0x8D || op == 0xFF || op == 0x31 || op == 0x33) {
        if (i >= n) return 0;
        unsigned char modrm = p[i];
        return (modrm >> 6) == 0 && (modrm & 7) == 5;
    }
    return 0;
}

static uint8_t *follow_thunks(uint8_t *p) {
    for (int i = 0; i < 8 && p; i++) {
        if (p[0] == 0x48 && p[1] == 0x8D && p[2] == 0xA4 && p[3] == 0x24 && p[4] == 0 && p[5] == 0 &&
            p[6] == 0 && p[7] == 0) {
            p += 8;
            continue;
        }
        if (p[0] == 0x90) {
            p++;
            continue;
        }
        if (p[0] == 0x66 && p[1] == 0x90) {
            p += 2;
            continue;
        }
        if (p[0] == 0xE9) {
            p = p + 5 + *(int32_t *)(p + 1);
            continue;
        }
        if (p[0] == 0xFF && p[1] == 0x25) {
            uint8_t *slot = p + 6 + *(int32_t *)(p + 2);
            p = *(uint8_t **)slot;
            continue;
        }
        break;
    }
    return p;
}

static void *mmap_near(uint8_t *target) {
    uintptr_t base = (uintptr_t)target & ~((uintptr_t)0xFFFF);
    for (long off = 0x10000; off < 0x40000000L; off += 0x10000) {
        for (int sign = -1; sign <= 1; sign += 2) {
            void *want = (void *)(base + (uintptr_t)(sign * off));
            void *p = mmap(want, TUXGT_PAGE, PROT_READ | PROT_WRITE | PROT_EXEC, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
            if (p == MAP_FAILED) continue;
            intptr_t rel = (uint8_t *)p - (target + 5);
            if (rel == (int32_t)rel) return p;
            munmap(p, TUXGT_PAGE);
        }
    }
    return NULL;
}

uint8_t *g_gated_fn[16];
int g_gated_n;

int install_gate(uint8_t *k32, const char *name, void *elf_gate, void **orig_out) {
    uint8_t *fn = (uint8_t *)pe_export(k32, name);
    if (!fn) return 0;
    fn = follow_thunks(fn);
    if (!fn) return 0;
    for (int i = 0; i < g_gated_n; i++)
        if (g_gated_fn[i] == fn) return 1;
    int sl = stolen_len(fn);
    if (sl < 5) return 0;
    unsigned stolen = (unsigned)sl;
    for (int off = 0; off < sl;) {
        int n = insn_len(fn + off);
        if (n <= 0) break;
        if (insn_is_rel(fn + off, n)) return 0;
        off += n;
    }
    uint8_t *stub = mmap_near(fn);
    if (!stub) return 0;
    memcpy(stub, fn, stolen);
    stub[stolen + 0] = 0x48;
    stub[stolen + 1] = 0xB8;
    *(uint64_t *)(stub + stolen + 2) = (uint64_t)(fn + stolen);
    stub[stolen + 10] = 0xFF;
    stub[stolen + 11] = 0xE0;
    uint8_t *gate = stub + 64;
    gate[0] = 0x48;
    gate[1] = 0xB8;
    *(uint64_t *)(gate + 2) = (uint64_t)elf_gate;
    gate[10] = 0xFF;
    gate[11] = 0xE0;
    uintptr_t page = (uintptr_t)fn & ~((uintptr_t)0xFFF);
    uintptr_t end = ((uintptr_t)fn + stolen + 16 + 0xFFF) & ~((uintptr_t)0xFFF);
    size_t span = end > page ? (size_t)(end - page) : TUXGT_PAGE;
    if (mprotect((void *)page, span, PROT_READ | PROT_WRITE | PROT_EXEC) != 0 &&
        mprotect((void *)page, TUXGT_PAGE, PROT_READ | PROT_WRITE | PROT_EXEC) != 0) {
        munmap(stub, TUXGT_PAGE);
        return 0;
    }
    intptr_t rel = gate - (fn + 5);
    uint8_t raw[8];
    memcpy(raw, fn, 8);
    raw[0] = 0xE9;
    *(int32_t *)(raw + 1) = (int32_t)rel;
    if (stolen > 5) {
        unsigned nop_end = stolen < 8 ? stolen : 8;
        for (unsigned i = 5; i < nop_end; i++) raw[i] = 0x90;
    }
    uint64_t patch;
    memcpy(&patch, raw, 8);
    *orig_out = stub;
    __atomic_thread_fence(__ATOMIC_RELEASE);
    if (stolen >= 8 && ((uintptr_t)fn & 7) == 0) {
        __atomic_store_n((uint64_t *)fn, patch, __ATOMIC_RELEASE);
        for (unsigned i = 8; i < stolen; i++) fn[i] = 0x90;
    } else {
        memcpy(fn, raw, stolen < 8 ? stolen : 8);
        if (stolen > 8) {
            for (unsigned i = 8; i < stolen; i++) fn[i] = 0x90;
        }
    }
    if (g_FlushIC) g_FlushIC((void *)(intptr_t)-1, fn, stolen);
    __builtin___clear_cache((char *)fn, (char *)fn + stolen);
    if (g_gated_n < 16) g_gated_fn[g_gated_n++] = fn;
    return 1;
}
