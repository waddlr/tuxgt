#define _GNU_SOURCE
#include "tuxgt-launcher.h"

void *TUXGT_WINABI gate_llw(const uint16_t *n) {
    load_wait();
    if (g_in_ll) return g_orig_llw(n);
    load_once("LoadLibraryW");
    return g_orig_llw(n);
}

void *TUXGT_WINABI gate_llew(const uint16_t *n, void *file, uint32_t flags) {
    load_wait();
    if (g_in_ll) return g_orig_llew(n, file, flags);
    load_once("LoadLibraryExW");
    return g_orig_llew(n, file, flags);
}

unsigned long TUXGT_WINABI gate_gtc(void) {
    load_once("GetTickCount");
    return g_orig_gtc();
}
unsigned long long TUXGT_WINABI gate_gtc64(void) {
    load_once("GetTickCount64");
    return g_orig_gtc64();
}
int TUXGT_WINABI gate_qpc(void *li) {
    load_once("QueryPerformanceCounter");
    return g_orig_qpc(li);
}
void TUXGT_WINABI gate_sleep(unsigned long ms) {
    load_once("Sleep");
    g_orig_sleep(ms);
}
unsigned long TUXGT_WINABI gate_sleepex(unsigned long ms, int alertable) {
    load_once("SleepEx");
    return g_orig_sleepex(ms, alertable);
}

// ModRM + displacement width. `addr16` is 0x67 (16-bit addressing) on
// i386 only: in 32-bit mode it swaps the disp32/rm=5 form for the
// disp16/rm=6 form and removes SIB. On x86-64, 0x67 still means 32-bit
// addressing, so the flag stays 0 and this is the original decoder.
static int modrm_size(const uint8_t *p, int addr16) {
    unsigned char modrm = p[0];
    int mod = modrm >> 6;
    int rm = modrm & 7;
    int n = 1;
    if (!addr16 && mod != 3 && rm == 4) {
        n++;
        unsigned char sib = p[1];
        if ((sib & 7) == 5 && mod == 0) n += 4;
    }
    if (mod == 1) n += 1;
    else if (mod == 2) n += addr16 ? 2 : 4;
    else if (mod == 0 && rm == 5 && !addr16) n += 4;
    else if (mod == 0 && rm == 6 && addr16) n += 2;
    return n;
}

// Length of one instruction. REX is x86-64 only: on i386 the 0x40-0x4F
// range is one-byte INC/DEC r32 and must not be skipped as a prefix.
// 0x66 (operand size) narrows the imm16/imm32 immediates.
static int insn_len(const uint8_t *p) {
    const uint8_t *s = p;
    int opsz16 = 0, addr16 = 0;
    // opsz16 only narrows immediates on i386; x86-64 keeps the original width.
    (void)opsz16;
    for (int i = 0; i < 4; i++) {
        unsigned char b = *p;
        if (b == 0x66) { opsz16 = 1; p++; continue; }
        if (b == 0x67) {
#if defined(__i386__)
            addr16 = 1;
#endif
            p++;
            continue;
        }
        if (b == 0xF0 || b == 0xF2 || b == 0xF3) { p++; continue; }
#if !defined(__i386__)
        if (b >= 0x40 && b <= 0x4F) { p++; break; }
#endif
        break;
    }
    // 0x66 (operand size) narrows the imm16/imm32 immediates. Scoped to
    // i386 on purpose: the x86-64 decoder has always counted a 0x66
    // immediate as imm32, and narrowing it there would change 64-bit
    // stolen_len. 64-bit keeps the original width.
#if defined(__i386__)
    int iw = opsz16 ? 2 : 4;
#else
    int iw = 4;
#endif
#if defined(__i386__)
    if ((unsigned char)*p >= 0x40 && (unsigned char)*p <= 0x4F) return (int)(p - s) + 1;
#endif
    unsigned char op = *p++;
    if (op == 0x0F) {
        unsigned char op2 = *p++;
        if (op2 == 0x1F || op2 == 0x0D) return (int)(p - s) + modrm_size(p, addr16);
        if (op2 >= 0x80 && op2 <= 0x8F) return (int)(p - s) + 4;
        return (int)(p - s) + modrm_size(p, addr16);
    }
    if ((op & 0xF0) == 0x50) return (int)(p - s);
    if (op == 0x90 || op == 0x9C || op == 0x9D || op == 0xCC || op == 0xC3) return (int)(p - s);
    if (op == 0x6A) return (int)(p - s) + 1;
    if (op == 0x68) return (int)(p - s) + iw;
    if (op == 0xE8 || op == 0xE9) return (int)(p - s) + 4;
    if (op == 0xEB) return (int)(p - s) + 1;
    if (op == 0xC2) return (int)(p - s) + 2;
    if (op == 0x83) return (int)(p - s) + modrm_size(p, addr16) + 1;
    if (op == 0x81) return (int)(p - s) + modrm_size(p, addr16) + iw;
    if (op == 0xC6) return (int)(p - s) + modrm_size(p, addr16) + 1;
    if (op == 0xC7) return (int)(p - s) + modrm_size(p, addr16) + iw;
    if (op == 0x84 || op == 0x85 || op == 0x88 || op == 0x89 || op == 0x8A || op == 0x8B || op == 0x8D ||
        op == 0xFF) {
        return (int)(p - s) + modrm_size(p, addr16);
    }
    if (op >= 0x70 && op <= 0x7F) return (int)(p - s) + 1;
    if (op >= 0xB8 && op <= 0xBF) {
        // x86-64 keeps the original REX test, which reads s[0] only: a REX
        // behind a legacy prefix is counted as imm32 there. Preserved
        // deliberately so 64-bit stolen_len is bit-identical.
#if !defined(__i386__)
        int rex_w = (s[0] >= 0x48 && s[0] <= 0x4F && (s[0] & 8));
        return (int)(p - s) + (rex_w ? 8 : 4);
#else
        return (int)(p - s) + iw;
#endif
    }
    if (op == 0x31 || op == 0x33) return (int)(p - s) + modrm_size(p, addr16);
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

// Absolute-memory ModRM, which cannot be copied into a stub at a different
// address. x86-64 refuses the RIP-relative rm=5 form. i386 has no RIP, so
// the absolute form is a plain disp32 rm=5 plus, under 0x67 addressing, the
// 16-bit rm=6 form. Scoped to i386: 64-bit keeps rm=5 only.
static int abs_rm(unsigned char modrm, const uint8_t *p) {
#if defined(__i386__)
    int rm = modrm & 7;
    return rm == 5 || (rm == 6 && p[0] == 0x67);
#else
    (void)p;
    return (modrm & 7) == 5;
#endif
}

// Relative call/jmp/jcc or position-dependent ModRM cannot be copied into
// the stub. On i386 the 0x40-0x4F range is INC/DEC, not REX, so it is
// excluded from the prefix skip; rm=5 (disp32) and, under 0x67, rm=6
// (disp16) are the absolute-memory forms there.
static int insn_is_rel(const uint8_t *p, int n) {
    int i = 0;
#if defined(__i386__)
    while (i < n && (p[i] == 0x66 || p[i] == 0x67 || p[i] == 0xF0 || p[i] == 0xF2 || p[i] == 0xF3))
        i++;
#else
    while (i < n && (p[i] == 0x66 || p[i] == 0x67 || p[i] == 0xF0 || p[i] == 0xF2 || p[i] == 0xF3 ||
                     (p[i] >= 0x40 && p[i] <= 0x4F)))
        i++;
#endif
    if (i >= n) return 0;
    unsigned char op = p[i++];
    if (op == 0xE8 || op == 0xE9 || op == 0xEB || (op >= 0x70 && op <= 0x7F)) return 1;
    if (op == 0x0F) {
        if (i >= n) return 0;
        unsigned char op2 = p[i++];
        if (op2 >= 0x80 && op2 <= 0x8F) return 1;
        if (i >= n) return 0;
        unsigned char modrm = p[i];
        return (modrm >> 6) == 0 && (abs_rm(modrm, p));
    }
    if (op == 0x83 || op == 0x81 || op == 0xC6 || op == 0xC7 || op == 0x84 || op == 0x85 || op == 0x88 ||
        op == 0x89 || op == 0x8A || op == 0x8B || op == 0x8D || op == 0xFF || op == 0x31 || op == 0x33) {
        if (i >= n) return 0;
        unsigned char modrm = p[i];
        return (modrm >> 6) == 0 && (abs_rm(modrm, p));
    }
    return 0;
}

// Stub jump forms. i386 has no 64-bit register: an absolute jump is
// `B8 <imm32>` + `FF E0` (mov eax, imm32; jmp eax) = 7 bytes, the same
// reg/reg shape as the x86-64 `movabs rax; jmp rax` pair. Written into a
// 32-bit scratch page, so the immediate is a plain low-32 address.
static uint8_t *emit_abs_jump(uint8_t *p, const void *target) {
#if defined(__i386__)
    uint32_t v = (uint32_t)(uintptr_t)target;
    p[0] = 0xB8;
    memcpy(p + 1, &v, 4);
    p[5] = 0xFF;
    p[6] = 0xE0;
    return p + 7;
#else
    p[0] = 0x48;
    p[1] = 0xB8;
    uint64_t v = (uint64_t)(uintptr_t)target;
    memcpy(p + 2, &v, 8);
    p[10] = 0xFF;
    p[11] = 0xE0;
    return p + 12;
#endif
}

static uint8_t *follow_thunks(uint8_t *p) {
    for (int i = 0; i < 8 && p; i++) {
#if !defined(__i386__)
        // REX `lea r12, [rsp]` hotpatch prolog: x86-64 only.
        if (p[0] == 0x48 && p[1] == 0x8D && p[2] == 0xA4 && p[3] == 0x24 && p[4] == 0 && p[5] == 0 &&
            p[6] == 0 && p[7] == 0) {
            p += 8;
            continue;
        }
#endif
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
            // `jmp [disp32]`: on x86-64 the displacement is RIP-relative
            // (slot = p + 6 + disp); on i386 it is an absolute address, so
            // the slot address is read from the 4 bytes at p + 2.
#if defined(__i386__)
            uint8_t *slot;
            memcpy(&slot, p + 2, sizeof(slot));
#else
            uint8_t *slot = p + 6 + *(int32_t *)(p + 2);
#endif
            p = *(uint8_t **)slot;
            continue;
        }
        break;
    }
    return p;
}

#if defined(__i386__)
// i386 needs a low-memory scratch page for the absolute-jump immediates;
// MAP_32BIT keeps the search short. rel32 reaches anywhere in 32-bit
// space, so the gate itself carries no near-placement constraint.
static void *mmap_near32(void) {
    void *p = mmap(NULL, TUXGT_PAGE, PROT_READ | PROT_WRITE | PROT_EXEC,
                   MAP_PRIVATE | MAP_ANONYMOUS | MAP_32BIT, -1, 0);
    return p == MAP_FAILED ? NULL : p;
}
#endif

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
#if defined(__i386__)
    // rel32 from the victim reaches anywhere in 32-bit space, so the gate
    // has no near-placement constraint; only the stub's own absolute-jump
    // immediates must be low-32, which any i386 mapping satisfies.
    if (!stub) stub = mmap_near32();
#endif
    if (!stub) return 0;
    memcpy(stub, fn, stolen);
    emit_abs_jump(stub + stolen, fn + stolen);
    uint8_t *gate = stub + 64;
    emit_abs_jump(gate, elf_gate);
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
    *orig_out = stub;
    __atomic_thread_fence(__ATOMIC_RELEASE);
#if defined(__i386__)
    // No 8-byte atomic store on i386: write the 5-byte rel32 jump, then
    // NOP the rest of the stolen bytes. Gates arm once, before any thread
    // can reach the victim, matching the 64-bit non-atomic fallback path.
    memcpy(fn, raw, 5);
    for (unsigned i = 5; i < stolen; i++) fn[i] = 0x90;
#else
    uint64_t patch;
    memcpy(&patch, raw, 8);
    if (stolen >= 8 && ((uintptr_t)fn & 7) == 0) {
        __atomic_store_n((uint64_t *)fn, patch, __ATOMIC_RELEASE);
        for (unsigned i = 8; i < stolen; i++) fn[i] = 0x90;
    } else {
        memcpy(fn, raw, stolen < 8 ? stolen : 8);
        if (stolen > 8) {
            for (unsigned i = 8; i < stolen; i++) fn[i] = 0x90;
        }
    }
#endif
    if (g_FlushIC) g_FlushIC((void *)(intptr_t)-1, fn, stolen);
    __builtin___clear_cache((char *)fn, (char *)fn + stolen);
    if (g_gated_n < 16) g_gated_fn[g_gated_n++] = fn;
    return 1;
}
