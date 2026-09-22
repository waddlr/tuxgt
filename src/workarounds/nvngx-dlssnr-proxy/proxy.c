/*
 * nvngx_dlssnr proxy DLL.
 *
 * Rename the real DLSS-NR snippet to "nvngx_dlssnr.impl.dll" and
 * drop this proxy as "nvngx_dlssnr.dll". It exports the full NGX
 * surface (the same 55 names the real DLL exposes, per
 * llvm-readobj --coff-exports) and forwards every call to the
 * implementation via GetProcAddress.
 *
 * On the first NVSDK_NGX_*_Init / Init_Ext, the proxy IAT-patches
 * the impl's KERNEL32 GetModuleFileNameW / GetModuleFileNameA so
 * the snippet sees C:\windows\system32\nvngx.dll instead of its
 * own real path. While NVSDK_NGX_*_CreateFeature / EvaluateFeature
 * runs, a TLS flag forces the spoof on regardless of which module
 * asks.
 *
 * This file is C99 plus __thread (native TLS), no C++.
 */

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdarg.h>
#include <stdint.h>
#include <string.h>

/* NGX result type. */
typedef uint32_t NGX_RESULT;
#define NGX_FAIL 0xBAD00000u
#define NGX_FAIL_FEATURE_NOT_SUPPORTED 0xBAD00001u
#define NGX_FAIL_PLATFORM_ERROR        0xBAD00002u

/* Opaque D3D handle types. The impl DLL receives the same
   pointers the caller passed, so casting at the boundary is safe. */
typedef void NGX_D3D12_DEVICE;
typedef void NGX_D3D12_CMDLIST;
typedef void NGX_D3D11_DEVICE;
typedef void NGX_D3D11_CMDLIST;

#pragma pack(push, 8)
struct NGX_LoggingInfo {
    void* LoggingCallback;
    int   MinimumLoggingLevel;
    int   DisableOtherLoggingSinks;
};
struct NGX_FeatureCommonInfo {
    const wchar_t** PathList;
    unsigned        PathListSize;
    struct NGX_LoggingInfo LoggingInfo;
    const void*     Internal;
};
#pragma pack(pop)

/* TLS flag: 1 while we are inside NVSDK_NGX_*_Init_Ext / Init /
   CreateFeature / EvaluateFeature (Init because the runtime's
   identity check fires during DllMain-style init; Create/Evaluate
   because the NR feature itself does the same check). Outside those
   calls the hook returns the real path so the snippet can do things
   like D3D12CreateDevice(nullptr) which need the true exe path. */
static __thread int g_in_ngx_call = 0;

/* Nesting counter: a reentrant same-thread call must not clear an outer window. */
static void enter_ngx(void) { g_in_ngx_call++; }
static void leave_ngx(void) { if (g_in_ngx_call > 0) g_in_ngx_call--; }

static HMODULE g_impl     = NULL;
static int     g_iat_done = 0;
static wchar_t g_log_path[MAX_PATH];

/* Real function pointers captured before the IAT is patched. */
static DWORD (WINAPI* g_orig_gmfwW)(HMODULE, LPWSTR, DWORD) = NULL;
static DWORD (WINAPI* g_orig_gmfwA)(HMODULE, LPSTR,  DWORD) = NULL;

/* ---------------------------------------------------------------- */
/* logging                                                           */
/* ---------------------------------------------------------------- */

static void log_line(const char* fmt, ...) {
    FILE* f = _wfopen(g_log_path, L"a");
    if (!f) return;
    va_list ap;
    va_start(ap, fmt);
    vfprintf(f, fmt, ap);
    va_end(ap);
    fputc('\n', f);
    fclose(f);
}

static void build_log_path(void) {
    HMODULE self = NULL;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
                            GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            (LPCWSTR)&build_log_path, &self)) {
        wcscpy(g_log_path, L"nvngx_dlssnr.log");
        return;
    }
    if (!GetModuleFileNameW(self, g_log_path, MAX_PATH)) {
        wcscpy(g_log_path, L"nvngx_dlssnr.log");
        return;
    }
    wchar_t* dot = wcsrchr(g_log_path, L'.');
    if (dot && (size_t)(dot - g_log_path) + 5 <= MAX_PATH) wcscpy(dot, L".log");
    else if (!dot && wcslen(g_log_path) + 5 <= MAX_PATH) wcscat(g_log_path, L".log");
    else wcscpy(g_log_path, L"nvngx_dlssnr.log");
}

/* ---------------------------------------------------------------- */
/* IAT patching helper (mirrors src/hook/ngx.cpp:iat_hook)           */
/* ---------------------------------------------------------------- */

static int iat_hook_one(HMODULE mod, const char* dll, const char* fn, void* hook, void** orig) {
    uint8_t* base = (uint8_t*)mod;
    IMAGE_DOS_HEADER* dos = (IMAGE_DOS_HEADER*)base;
    if (dos->e_magic != IMAGE_DOS_SIGNATURE) return 0;
    IMAGE_NT_HEADERS* nt = (IMAGE_NT_HEADERS*)(base + dos->e_lfanew);
    if (nt->Signature != IMAGE_NT_SIGNATURE) return 0;
    IMAGE_DATA_DIRECTORY dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    if (!dir.VirtualAddress) return 0;
    IMAGE_IMPORT_DESCRIPTOR* imp = (IMAGE_IMPORT_DESCRIPTOR*)(base + dir.VirtualAddress);
    for (; imp->Name; ++imp) {
        const char* name = (const char*)(base + imp->Name);
        if (_stricmp(name, dll) != 0) continue;
        if (!imp->OriginalFirstThunk) continue; /* bound image: no names to match */
        IMAGE_THUNK_DATA* iat = (IMAGE_THUNK_DATA*)(base + imp->FirstThunk);
        IMAGE_THUNK_DATA* ilt = (IMAGE_THUNK_DATA*)(base + imp->OriginalFirstThunk);
        for (; iat->u1.Function; ++iat, ++ilt) {
            if (IMAGE_SNAP_BY_ORDINAL(ilt->u1.Ordinal)) continue;
            IMAGE_IMPORT_BY_NAME* ibn = (IMAGE_IMPORT_BY_NAME*)(base + ilt->u1.AddressOfData);
            if (strcmp((const char*)ibn->Name, fn) != 0) continue;
            DWORD old = 0;
            if (!VirtualProtect(&iat->u1.Function, sizeof(iat->u1.Function), PAGE_READWRITE, &old)) return 0;
            if (orig) *orig = (void*)iat->u1.Function;
            iat->u1.Function = (ULONG_PTR)hook;
            VirtualProtect(&iat->u1.Function, sizeof(iat->u1.Function), old, &old);
            return 1;
        }
    }
    return 0;
}

/* ---------------------------------------------------------------- */
/* GMFW hooks                                                        */
/* ---------------------------------------------------------------- */

static const wchar_t kSpoofW[] = L"C:\\windows\\system32\\nvngx.dll";
static const char    kSpoofA[] = "C:\\windows\\system32\\nvngx.dll";

static int is_impl_handle(HMODULE h, const wchar_t* scratch) {
    /* The impl is allowed to ask for its own path so it can find
       itself; that is the only call we leave untouched. */
    if (h == g_impl) return 1;
    if (scratch && g_impl) {
        wchar_t impl_path[MAX_PATH];
        if (GetModuleFileNameW(g_impl, impl_path, MAX_PATH)) {
            /* leaf-only match: the impl may pass any handle and still
               be checking itself. */
            const wchar_t* s1 = NULL, *s2 = NULL;
            for (const wchar_t* q = impl_path; *q; ++q)
                if (*q == L'\\' || *q == L'/') s1 = q + 1;
            for (const wchar_t* q = scratch;   *q; ++q)
                if (*q == L'\\' || *q == L'/') s2 = q + 1;
            if (s1 && s2 && _wcsicmp(s1, s2) == 0) return 1;
        }
    }
    return 0;
}

static DWORD WINAPI hooked_gmfwW(HMODULE h, LPWSTR buf, DWORD n) {
    if (buf == NULL || n == 0) {
        return g_orig_gmfwW ? g_orig_gmfwW(h, buf, n) : 0;
    }
    wchar_t scratch[MAX_PATH] = {0};
    DWORD got = g_orig_gmfwW ? g_orig_gmfwW(h, scratch, MAX_PATH) : 0;
    /* Transcript final rule (ACBFR run that returned Init_Ext 0x1
       then CreateFeature 18 0x1): spoof every call except the impl
       asking for its own path, AND only while we are inside an NGX
       Init_Ext/CreateFeature/EvaluateFeature. Outside those calls
       the snippet does D3D12CreateDevice(nullptr) on the real exe
       path and we must not lie. */
    int should_spoof = g_in_ngx_call && !is_impl_handle(h, scratch);
    if (should_spoof) {
        wcsncpy(buf, kSpoofW, n);
        buf[n - 1] = 0;
        return (DWORD)wcslen(buf);
    }
    if (got > 0) {
        wcsncpy(buf, scratch, n);
        buf[n - 1] = 0;
        return (DWORD)wcslen(buf);
    }
    return got;
}

static DWORD WINAPI hooked_gmfwA(HMODULE h, LPSTR buf, DWORD n) {
    if (buf == NULL || n == 0) {
        return g_orig_gmfwA ? g_orig_gmfwA(h, buf, n) : 0;
    }
    char scratch[MAX_PATH];
    DWORD got = g_orig_gmfwA ? g_orig_gmfwA(h, scratch, MAX_PATH) : 0;
    /* Mirror hooked_gmfwW: only the impl asking for its own path is
       preserved, and only while we are inside an NGX call. */
    wchar_t scratch_w[MAX_PATH] = {0};
    if (got > 0) {
        scratch[MAX_PATH - 1] = 0; /* bound the -1 scan if the API truncated */
        MultiByteToWideChar(CP_ACP, 0, scratch, -1, scratch_w, MAX_PATH);
    }
    int should_spoof = g_in_ngx_call && !is_impl_handle(h, scratch_w);
    if (should_spoof) {
        strncpy(buf, kSpoofA, n);
        buf[n - 1] = 0;
        return (DWORD)strlen(buf);
    }
    if (got > 0) {
        strncpy(buf, scratch, n);
        buf[n - 1] = 0;
        return (DWORD)strlen(buf);
    }
    return got;
}

/* ---------------------------------------------------------------- */
/* impl mapping + IAT install                                        */
/* ---------------------------------------------------------------- */

static void install_gmfw_iat(void) {
    if (g_iat_done || !g_impl) return;
    if (iat_hook_one(g_impl, "KERNEL32.dll", "GetModuleFileNameW",
                     (void*)hooked_gmfwW, (void**)&g_orig_gmfwW)) {
        log_line("IAT hooked KERNEL32!GetModuleFileNameW in impl");
    } else {
        log_line("IAT hook KERNEL32!GetModuleFileNameW FAILED");
    }
    if (iat_hook_one(g_impl, "KERNEL32.dll", "GetModuleFileNameA",
                     (void*)hooked_gmfwA, (void**)&g_orig_gmfwA)) {
        log_line("IAT hooked KERNEL32!GetModuleFileNameA in impl");
    } else {
        log_line("IAT hook KERNEL32!GetModuleFileNameA FAILED");
    }
    g_iat_done = 1;
}

static LONG g_ensure_lock = 0;
static void load_impl(void); /* defined below */

/* Retry a failed attach-time load on a later Init. LoadLibrary outside
   DllMain is loader-safe; the try-lock keeps concurrent retries from
   double-installing the IAT hook (loser returns, next call retries). */
static void ensure_impl(void) {
    if (g_impl) return;
    if (InterlockedCompareExchange(&g_ensure_lock, 1, 0) != 0) return;
    if (!g_impl) load_impl();
    install_gmfw_iat();
    InterlockedExchange(&g_ensure_lock, 0);
}

static int self_path(wchar_t* out) {
    HMODULE self = NULL;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
                            GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            (LPCWSTR)&self_path, &self))
        return 0;
    if (!GetModuleFileNameW(self, out, MAX_PATH))
        return 0;
    return 1;
}

static void load_impl(void) {
    /* Game dir currently keeps the snippet as nvngx_dlssnr.real.dll. */
    static const wchar_t* leaves[] = { L"nvngx_dlssnr.impl.dll", L"nvngx_dlssnr.real.dll" };
    wchar_t self[MAX_PATH];
    unsigned i;
    /* Sibling first: the backend travels with the proxy (staged
       runtime dir, exe dir, anywhere), the same way ReShade resolves
       beside its own module. Bare-name fallback keeps exe-dir / PATH /
       already-loaded layouts working. */
    if (self_path(self)) {
        wchar_t* slash = wcsrchr(self, L'\\');
        if (!slash) slash = wcsrchr(self, L'/');
        if (slash) {
            size_t dirlen = (size_t)(slash - self) + 1;
            for (i = 0; i < 2 && !g_impl; i++) {
                if (dirlen + wcslen(leaves[i]) < MAX_PATH) {
                    wchar_t sibling[MAX_PATH];
                    wcsncpy(sibling, self, dirlen);
                    sibling[dirlen] = 0;
                    wcscat(sibling, leaves[i]);
                    g_impl = LoadLibraryW(sibling);
                    if (!g_impl)
                        log_line("LoadLibraryW %ls FAILED err=%lu", sibling, GetLastError());
                }
            }
        }
    }
    for (i = 0; i < 2 && !g_impl; i++) {
        g_impl = LoadLibraryW(leaves[i]);
        if (!g_impl)
            log_line("LoadLibraryW %ls FAILED err=%lu", leaves[i], GetLastError());
    }
    if (g_impl) {
        wchar_t path[MAX_PATH];
        GetModuleFileNameW(g_impl, path, MAX_PATH);
        log_line("loaded impl @ %p path=%ls", g_impl, path);
    }
}

/* ---------------------------------------------------------------- */
/* Generic forwarder: a 5-slot C-callable thunk that re-resolves     */
/* the impl export on every call. Uses the x64 register convention    */
/* where the first 4 args live in RCX/RDX/R8/R9 and the 5th on the   */
/* stack. Declaring one prototype with five pointer-sized args is    */
/* enough for every signature the real snippet exposes.              */
/* ---------------------------------------------------------------- */

typedef NGX_RESULT (__cdecl* fwd5_t)(void*, void*, void*, void*, void*);
typedef NGX_RESULT (__cdecl* fwd4_t)(void*, void*, void*, void*);
typedef NGX_RESULT (__cdecl* fwd1_t)(void*);

static NGX_RESULT __cdecl call5(const char* name,
                                 void* a, void* b, void* c, void* d, void* e) {
    if (!g_impl) return NGX_FAIL_PLATFORM_ERROR;
    fwd5_t f = (fwd5_t)GetProcAddress(g_impl, name);
    return f ? (NGX_RESULT)f(a, b, c, d, e) : NGX_FAIL_FEATURE_NOT_SUPPORTED;
}
/* Snippet entries (Populate, Release, Shutdown, Init_Ext) call
 * GetModuleHandleExA(FROM_ADDRESS) on their return address, then
 * GetModuleFileNameW, and reject the caller unless that path contains
 * "nvngx.dll". The return address is this proxy (nvngx_dlssnr.dll),
 * which fails that test with 0xBAD00002. The spoof flag has to be set
 * for the whole forward, not only Init/Create/Evaluate. */
static NGX_RESULT __cdecl call0(const char* name) {
    NGX_RESULT r;
    int prev;
    typedef NGX_RESULT (__cdecl* f_t)(void);
    f_t f;
    if (!g_impl) return NGX_FAIL_PLATFORM_ERROR;
    f = (f_t)GetProcAddress(g_impl, name);
    if (!f) return NGX_FAIL_FEATURE_NOT_SUPPORTED;
    prev = g_in_ngx_call;
    g_in_ngx_call = 1;
    r = f();
    g_in_ngx_call = prev;
    return r;
}
static NGX_RESULT __cdecl call1(const char* name, void* a) {
    NGX_RESULT r;
    int prev;
    fwd1_t f;
    if (!g_impl) return NGX_FAIL_PLATFORM_ERROR;
    f = (fwd1_t)GetProcAddress(g_impl, name);
    if (!f) return NGX_FAIL_FEATURE_NOT_SUPPORTED;
    prev = g_in_ngx_call;
    g_in_ngx_call = 1;
    r = f(a);
    g_in_ngx_call = prev;
    return r;
}
static NGX_RESULT __cdecl call3(const char* name, void* a, void* b, void* c) {
    NGX_RESULT r;
    int prev;
    typedef NGX_RESULT (__cdecl* f_t)(void*, void*, void*);
    f_t f;
    if (!g_impl) return NGX_FAIL_PLATFORM_ERROR;
    f = (f_t)GetProcAddress(g_impl, name);
    if (!f) return NGX_FAIL_FEATURE_NOT_SUPPORTED;
    prev = g_in_ngx_call;
    g_in_ngx_call = 1;
    r = f(a, b, c);
    g_in_ngx_call = prev;
    return r;
}

/* ---------------------------------------------------------------- */
/* ReShade device identity.                                          */
/* ReShade stores its D3D12Device proxy on the vkd3d device under    */
/* this UUID and makes ID3D12Resource::GetDevice return that proxy.  */
/* The proxy's CPU descriptors are not vkd3d handles. DFC passes the */
/* raw device into NGX, NGX then GetDevice's a game resource and     */
/* gets the proxy, and CreateFeature faults in both encodings        */
/* (dxgi.dll RVA 0x148512, d3d12core.dll RVA 0x42B20A). Clearing the */
/* private data for the call keeps GetDevice on vkd3d. A ReShade     */
/* command-list proxy is unwrapped the same way (IID below).         */
/* ---------------------------------------------------------------- */

static const GUID k_iid_unwrap =
    { 0x7f2c9a11, 0x3b4e, 0x4d6a, { 0x81, 0x2f, 0x5e, 0x9c, 0xd3, 0x7a, 0x1b, 0x42 } };
static const GUID k_iid_rs_dev =
    { 0x2523aff4, 0x978b, 0x4939, { 0xba, 0x16, 0x8e, 0xe8, 0x76, 0xa4, 0xcb, 0x2a } };
static const GUID k_iid_d3d12_dev =
    { 0x189819f1, 0x1db6, 0x4b57, { 0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7 } };

typedef HRESULT (WINAPI *pfn_qi)(void*, const GUID*, void**);
typedef ULONG   (WINAPI *pfn_rel)(void*);
typedef HRESULT (WINAPI *pfn_gpd)(void*, const GUID*, UINT*, void*);
typedef HRESULT (WINAPI *pfn_spd)(void*, const GUID*, UINT, const void*);
typedef HRESULT (WINAPI *pfn_gd)(void*, const GUID*, void**);

static __thread int   g_rs_depth = 0;
static __thread void* g_rs_dev = NULL;
static __thread void* g_rs_proxy = NULL;
static __thread void* g_hold[4];
static __thread int   g_hold_n = 0;
static int g_rs_logs = 0;
static void* g_veh = NULL;

static int com_image(const void* p) {
    MEMORY_BASIC_INFORMATION mbi;
    if (!p || !VirtualQuery(p, &mbi, sizeof mbi)) return 0;
    return mbi.State == MEM_COMMIT && mbi.Type == MEM_IMAGE &&
           !(mbi.Protect & (PAGE_NOACCESS | PAGE_GUARD));
}
static int com_vt(void* obj, void*** out) {
    void** vt;
    MEMORY_BASIC_INFORMATION mbi;
    if (!obj || !out) return 0;
    if (!VirtualQuery(obj, &mbi, sizeof mbi)) return 0;
    if (mbi.State != MEM_COMMIT || (mbi.Protect & (PAGE_NOACCESS | PAGE_GUARD))) return 0;
    vt = *(void***)obj;
    if (!com_image(vt) || !com_image(vt[0])) return 0;
    *out = vt;
    return 1;
}
static void* d3d_unwrap(void* obj) {
    void** vt;
    void* raw = NULL;
    HRESULT hr;
    if (!com_vt(obj, &vt)) return obj;
    hr = ((pfn_qi)vt[0])(obj, &k_iid_unwrap, &raw);
    if (hr < 0 || !raw || raw == obj) return obj;
    return raw;
}
static void* list_device(void* list) {
    void** vt;
    void* dev = NULL;
    HRESULT hr;
    if (!com_vt(list, &vt) || !com_image(vt[7])) return NULL;
    hr = ((pfn_gd)vt[7])(list, &k_iid_d3d12_dev, &dev);
    if (hr < 0 || !dev) return NULL;
    return dev;
}
static void com_release(void* obj) {
    void** vt;
    if (!obj || !com_vt(obj, &vt) || !com_image(vt[2])) return;
    ((pfn_rel)vt[2])(obj);
}
static void hold_add(void* p) {
    if (p && g_hold_n < 4) g_hold[g_hold_n++] = p;
}
static void hold_release_to(int mark) {
    while (g_hold_n > mark) {
        g_hold_n--;
        com_release(g_hold[g_hold_n]);
        g_hold[g_hold_n] = NULL;
    }
}
static void rs_hide(void* device) {
    void** vt;
    UINT sz;
    void* proxy = NULL;
    HRESULT hr;
    if (g_rs_depth > 0) { g_rs_depth++; return; }
    g_rs_depth = 1;
    g_rs_dev = NULL;
    g_rs_proxy = NULL;
    if (!com_vt(device, &vt) || !com_image(vt[3]) || !com_image(vt[4])) return;
    sz = (UINT)sizeof(proxy);
    hr = ((pfn_gpd)vt[3])(device, &k_iid_rs_dev, &sz, &proxy);
    if (hr < 0 || !proxy || sz != sizeof(proxy)) return;
    hr = ((pfn_spd)vt[4])(device, &k_iid_rs_dev, 0, NULL);
    if (hr < 0) {
        if (g_rs_logs < 4)
            log_line("reshade proxy hide FAILED 0x%08lX dev=%p", (unsigned long)hr, device);
        g_rs_logs++;
        return;
    }
    g_rs_dev = device;
    g_rs_proxy = proxy;
    if (g_rs_logs < 4)
        log_line("reshade D3D12 proxy %p hidden (device %p)", proxy, device);
    g_rs_logs++;
}
static void rs_show(void) {
    void** vt;
    if (g_rs_depth <= 0) return;
    g_rs_depth--;
    if (g_rs_depth > 0) return;
    if (g_rs_dev && g_rs_proxy && com_vt(g_rs_dev, &vt) && com_image(vt[4])) {
        ((pfn_spd)vt[4])(g_rs_dev, &k_iid_rs_dev, (UINT)sizeof(g_rs_proxy), &g_rs_proxy);
        if (g_rs_logs < 4)
            log_line("reshade D3D12 proxy %p restored", g_rs_proxy);
    }
    g_rs_dev = NULL;
    g_rs_proxy = NULL;
}
static void rs_abort(void) {
    while (g_rs_depth > 0) rs_show();
    hold_release_to(0);
    g_in_ngx_call = 0;
}
static LONG CALLBACK rs_veh(EXCEPTION_POINTERS* e) {
    if (e->ExceptionRecord->ExceptionCode != (DWORD)0xC0000005) return EXCEPTION_CONTINUE_SEARCH;
    if (g_rs_depth > 0 || g_hold_n > 0) rs_abort();
    return EXCEPTION_CONTINUE_SEARCH;
}
static void* d3d12_enter(void* list, int* mark) {
    void* raw = d3d_unwrap(list);
    void* dev;
    *mark = g_hold_n;
    if (raw != list) hold_add(raw);
    dev = list_device(raw);
    if (dev) hold_add(dev);
    rs_hide(dev ? dev : raw);
    return raw;
}
static void d3d12_leave(int mark) {
    rs_show();
    hold_release_to(mark);
}

/* ---------------------------------------------------------------- */
/* Tracked forwarders: install IAT, gate g_in_create around eval.     */
/* These need the right signature so the wrapper reads registers     */
/* correctly. The others use the generic 5/4/1-arg helpers.          */
/* ---------------------------------------------------------------- */

NGX_RESULT __cdecl NVSDK_NGX_D3D12_Init(unsigned long long app,
                                         const wchar_t* data,
                                         NGX_D3D12_DEVICE* dev,
                                         struct NGX_FeatureCommonInfo* fi,
                                         uint32_t ver) {
    void* raw;
    int mark;
    NGX_RESULT r;
    ensure_impl();
    raw = d3d_unwrap(dev);
    mark = g_hold_n;
    if (raw != (void*)dev) hold_add(raw);
    rs_hide(raw);
    enter_ngx();
    r = call5("NVSDK_NGX_D3D12_Init",
              (void*)(uintptr_t)app, (void*)data, raw, fi, (void*)(uintptr_t)ver);
    leave_ngx();
    d3d12_leave(mark);
    return r;
}
NGX_RESULT __cdecl NVSDK_NGX_D3D12_Init_Ext(unsigned long long app,
                                             const wchar_t* data,
                                             NGX_D3D12_DEVICE* dev,
                                             uint32_t ver,
                                             struct NGX_FeatureCommonInfo* fi) {
    void* raw;
    int mark;
    NGX_RESULT r;
    ensure_impl();
    raw = d3d_unwrap(dev);
    mark = g_hold_n;
    if (raw != (void*)dev) hold_add(raw);
    rs_hide(raw);
    enter_ngx();
    r = call5("NVSDK_NGX_D3D12_Init_Ext",
              (void*)(uintptr_t)app, (void*)data, raw, (void*)(uintptr_t)ver, fi);
    leave_ngx();
    d3d12_leave(mark);
    log_line("D3D12 Init_Ext dev=%p raw=%p -> 0x%08X", (void*)dev, raw, r);
    return r;
}
NGX_RESULT __cdecl NVSDK_NGX_D3D11_Init(unsigned long long app,
                                         const wchar_t* data,
                                         NGX_D3D11_DEVICE* dev,
                                         struct NGX_FeatureCommonInfo* fi,
                                         uint32_t ver) {
    NGX_RESULT r;
    ensure_impl();
    enter_ngx();
    r = call5("NVSDK_NGX_D3D11_Init",
              (void*)(uintptr_t)app, (void*)data, dev, fi, (void*)(uintptr_t)ver);
    leave_ngx();
    return r;
}
NGX_RESULT __cdecl NVSDK_NGX_D3D11_Init_Ext(unsigned long long app,
                                             const wchar_t* data,
                                             NGX_D3D11_DEVICE* dev,
                                             uint32_t ver,
                                             struct NGX_FeatureCommonInfo* fi) {
    NGX_RESULT r;
    ensure_impl();
    enter_ngx();
    r = call5("NVSDK_NGX_D3D11_Init_Ext",
              (void*)(uintptr_t)app, (void*)data, dev, (void*)(uintptr_t)ver, fi);
    leave_ngx();
    log_line("D3D11 Init_Ext dev=%p -> 0x%08X", (void*)dev, r);
    return r;
}

/* Tracked: also set the TLS flag. */
NGX_RESULT __cdecl NVSDK_NGX_D3D12_CreateFeature(NGX_D3D12_CMDLIST* list,
                                                  int feature, void* params, void** handle) {
    void* raw;
    int mark;
    NGX_RESULT r;
    fwd4_t f;
    if (!g_impl) return NGX_FAIL_PLATFORM_ERROR;
    f = (fwd4_t)GetProcAddress(g_impl, "NVSDK_NGX_D3D12_CreateFeature");
    if (!f) return NGX_FAIL_FEATURE_NOT_SUPPORTED;
    raw = d3d12_enter(list, &mark);
    enter_ngx();
    r = f(raw, (void*)(intptr_t)feature, params, handle);
    leave_ngx();
    d3d12_leave(mark);
    log_line("D3D12 CreateFeature feature=%d list=%p raw=%p -> 0x%08X handle=%p",
             feature, (void*)list, raw, r, handle ? *handle : NULL);
    return r;
}
NGX_RESULT __cdecl NVSDK_NGX_D3D12_EvaluateFeature(NGX_D3D12_CMDLIST* list,
                                                    void* handle, void* params, void* extra) {
    void* raw;
    int mark;
    NGX_RESULT r;
    fwd4_t f;
    static int n = 0;
    if (!g_impl) return NGX_FAIL_PLATFORM_ERROR;
    f = (fwd4_t)GetProcAddress(g_impl, "NVSDK_NGX_D3D12_EvaluateFeature");
    if (!f) return NGX_FAIL_FEATURE_NOT_SUPPORTED;
    raw = d3d12_enter(list, &mark);
    enter_ngx();
    r = f(raw, handle, params, extra);
    leave_ngx();
    d3d12_leave(mark);
    if (n < 4) {
        log_line("D3D12 EvaluateFeature list=%p raw=%p -> 0x%08X", (void*)list, raw, r);
        n++;
    }
    return r;
}
NGX_RESULT __cdecl NVSDK_NGX_D3D11_CreateFeature(NGX_D3D11_CMDLIST* list,
                                                  int feature, void* params, void** handle) {
    NGX_RESULT r;
    fwd4_t f;
    if (!g_impl) return NGX_FAIL_PLATFORM_ERROR;
    f = (fwd4_t)GetProcAddress(g_impl, "NVSDK_NGX_D3D11_CreateFeature");
    if (!f) return NGX_FAIL_FEATURE_NOT_SUPPORTED;
    enter_ngx();
    r = f(list, (void*)(intptr_t)feature, params, handle);
    leave_ngx();
    log_line("D3D11 CreateFeature feature=%d list=%p -> 0x%08X handle=%p",
             feature, (void*)list, r, handle ? *handle : NULL);
    return r;
}
NGX_RESULT __cdecl NVSDK_NGX_D3D11_EvaluateFeature(NGX_D3D11_CMDLIST* list,
                                                    void* handle, void* params, void* extra) {
    NGX_RESULT r;
    fwd4_t f;
    static int n = 0;
    if (!g_impl) return NGX_FAIL_PLATFORM_ERROR;
    f = (fwd4_t)GetProcAddress(g_impl, "NVSDK_NGX_D3D11_EvaluateFeature");
    if (!f) return NGX_FAIL_FEATURE_NOT_SUPPORTED;
    enter_ngx();
    r = f(list, handle, params, extra);
    leave_ngx();
    if (n < 4) {
        log_line("D3D11 EvaluateFeature list=%p -> 0x%08X", (void*)list, r);
        n++;
    }
    return r;
}

/* ---------------------------------------------------------------- */
/* Tracked Init for CUDA too (snippet re-uses same IAT page).        */
/* ---------------------------------------------------------------- */

NGX_RESULT __cdecl NVSDK_NGX_CUDA_Init(unsigned long long app,
                                        const wchar_t* data,
                                        void* dev,
                                        struct NGX_FeatureCommonInfo* fi,
                                        uint32_t ver) {
    NGX_RESULT r;
    ensure_impl();
    enter_ngx();
    r = call5("NVSDK_NGX_CUDA_Init",
              (void*)(uintptr_t)app, (void*)data, dev, fi, (void*)(uintptr_t)ver);
    leave_ngx();
    return r;
}
NGX_RESULT __cdecl NVSDK_NGX_CUDA_Init_Ext(unsigned long long app,
                                            const wchar_t* data,
                                            void* dev,
                                            uint32_t ver,
                                            struct NGX_FeatureCommonInfo* fi) {
    NGX_RESULT r;
    ensure_impl();
    enter_ngx();
    r = call5("NVSDK_NGX_CUDA_Init_Ext",
              (void*)(uintptr_t)app, (void*)data, dev, (void*)(uintptr_t)ver, fi);
    leave_ngx();
    return r;
}

/* ---------------------------------------------------------------- */
/* Unsupported-ABI stubs: the CUDA/Vulkan/SetCallback entries are   */
/* outside the D3D11/D3D12 deployment scope. They fail cleanly with */
/* NGX_FAIL_FEATURE_NOT_SUPPORTED (ABI-safe on x64 whatever the     */
/* true signature is) instead of forwarding NULLs into the impl.    */
/* ---------------------------------------------------------------- */

static NGX_RESULT __cdecl unsupported(const char* name) {
    static int n = 0;
    if (n < 4) {
        log_line("STUB %s called: not supported", name);
        n++;
    } else if (n == 4) {
        log_line("STUB: further unsupported-entry calls suppressed");
        n++;
    }
    return NGX_FAIL_FEATURE_NOT_SUPPORTED;
}

NGX_RESULT __cdecl NVSDK_NGX_CUDA_CreateFeature(void) { return unsupported("NVSDK_NGX_CUDA_CreateFeature"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_CreateFeature1(void) { return unsupported("NVSDK_NGX_CUDA_CreateFeature1"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_EvaluateFeature(void) { return unsupported("NVSDK_NGX_CUDA_EvaluateFeature"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_GetFeatureRequirements(void) { return unsupported("NVSDK_NGX_CUDA_GetFeatureRequirements"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_GetScratchBufferSize(void) { return unsupported("NVSDK_NGX_CUDA_GetScratchBufferSize"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_Init1(void) { return unsupported("NVSDK_NGX_CUDA_Init1"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_Init_Ext1(void) { return unsupported("NVSDK_NGX_CUDA_Init_Ext1"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_PopulateParameters_Impl(void) { return unsupported("NVSDK_NGX_CUDA_PopulateParameters_Impl"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_ReleaseFeature(void) { return unsupported("NVSDK_NGX_CUDA_ReleaseFeature"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_Shutdown(void) { return unsupported("NVSDK_NGX_CUDA_Shutdown"); }
NGX_RESULT __cdecl NVSDK_NGX_CUDA_Shutdown1(void) { return unsupported("NVSDK_NGX_CUDA_Shutdown1"); }

NGX_RESULT __cdecl NVSDK_NGX_D3D11_GetFeatureRequirements(void* a, void* b, void* c) {
    return call3("NVSDK_NGX_D3D11_GetFeatureRequirements", a, b, c);
}
NGX_RESULT __cdecl NVSDK_NGX_D3D11_GetScratchBufferSize(void* a, void* b, void* c) {
    return call3("NVSDK_NGX_D3D11_GetScratchBufferSize", a, b, c);
}
NGX_RESULT __cdecl NVSDK_NGX_D3D11_PopulateParameters_Impl(void* params) {
    NGX_RESULT r = call1("NVSDK_NGX_D3D11_PopulateParameters_Impl", params);
    log_line("D3D11 Populate %p -> 0x%08X", params, r);
    return r;
}
NGX_RESULT __cdecl NVSDK_NGX_D3D11_ReleaseFeature(void* handle) {
    return call1("NVSDK_NGX_D3D11_ReleaseFeature", handle);
}
NGX_RESULT __cdecl NVSDK_NGX_D3D11_Shutdown(void) {
    return call0("NVSDK_NGX_D3D11_Shutdown");
}
NGX_RESULT __cdecl NVSDK_NGX_D3D11_Shutdown1(void* dev) {
    return call1("NVSDK_NGX_D3D11_Shutdown1", dev);
}

/* These used to ignore the caller's arguments and pass NULL. */
NGX_RESULT __cdecl NVSDK_NGX_D3D12_GetFeatureRequirements(void* a, void* b, void* c) {
    return call3("NVSDK_NGX_D3D12_GetFeatureRequirements", a, b, c);
}
NGX_RESULT __cdecl NVSDK_NGX_D3D12_GetScratchBufferSize(void* a, void* b, void* c) {
    return call3("NVSDK_NGX_D3D12_GetScratchBufferSize", a, b, c);
}
NGX_RESULT __cdecl NVSDK_NGX_D3D12_PopulateParameters_Impl(void* params) {
    NGX_RESULT r = call1("NVSDK_NGX_D3D12_PopulateParameters_Impl", params);
    log_line("D3D12 Populate %p -> 0x%08X", params, r);
    return r;
}
NGX_RESULT __cdecl NVSDK_NGX_D3D12_ReleaseFeature(void* handle) {
    return call1("NVSDK_NGX_D3D12_ReleaseFeature", handle);
}
NGX_RESULT __cdecl NVSDK_NGX_D3D12_Shutdown(void) {
    return call0("NVSDK_NGX_D3D12_Shutdown");
}
NGX_RESULT __cdecl NVSDK_NGX_D3D12_Shutdown1(void* dev) {
    return call1("NVSDK_NGX_D3D12_Shutdown1", dev);
}

unsigned int __cdecl NVSDK_NGX_GetAPIVersion(void) {
    if (!g_impl) return 0;
    typedef unsigned int (__cdecl* fn_t)(void);
    fn_t f = (fn_t)GetProcAddress(g_impl, "NVSDK_NGX_GetAPIVersion");
    return f ? f() : 0;
}
unsigned int __cdecl NVSDK_NGX_GetApplicationId(void) {
    if (!g_impl) return 0;
    typedef unsigned int (__cdecl* fn_t)(void);
    fn_t f = (fn_t)GetProcAddress(g_impl, "NVSDK_NGX_GetApplicationId");
    return f ? f() : 0;
}
unsigned int __cdecl NVSDK_NGX_GetDriverVersionEx(void) {
    if (!g_impl) return 0;
    typedef unsigned int (__cdecl* fn_t)(void);
    fn_t f = (fn_t)GetProcAddress(g_impl, "NVSDK_NGX_GetDriverVersionEx");
    return f ? f() : 0;
}
unsigned int __cdecl NVSDK_NGX_GetGPUArchitecture(void) {
    if (!g_impl) return 0;
    typedef unsigned int (__cdecl* fn_t)(void);
    fn_t f = (fn_t)GetProcAddress(g_impl, "NVSDK_NGX_GetGPUArchitecture");
    return f ? f() : 0;
}
unsigned int __cdecl NVSDK_NGX_GetSnippetVersion(void) {
    if (!g_impl) return 0;
    typedef unsigned int (__cdecl* fn_t)(void);
    fn_t f = (fn_t)GetProcAddress(g_impl, "NVSDK_NGX_GetSnippetVersion");
    return f ? f() : 0;
}

NGX_RESULT __cdecl NVSDK_NGX_SetOverrideStatusCallback(void) { return unsupported("NVSDK_NGX_SetOverrideStatusCallback"); }
NGX_RESULT __cdecl NVSDK_NGX_SetRuntimeParamsCallback(void) { return unsupported("NVSDK_NGX_SetRuntimeParamsCallback"); }
NGX_RESULT __cdecl NVSDK_NGX_SetTelemetryEvaluateCallback(void) { return unsupported("NVSDK_NGX_SetTelemetryEvaluateCallback"); }

NGX_RESULT __cdecl NVSDK_NGX_VULKAN_CreateFeature(void) { return unsupported("NVSDK_NGX_VULKAN_CreateFeature"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_CreateFeature1(void) { return unsupported("NVSDK_NGX_VULKAN_CreateFeature1"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_EvaluateFeature(void) { return unsupported("NVSDK_NGX_VULKAN_EvaluateFeature"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_GetFeatureDeviceExtensionRequirements(void) { return unsupported("NVSDK_NGX_VULKAN_GetFeatureDeviceExtensionRequirements"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_GetFeatureInstanceExtensionRequirements(void) { return unsupported("NVSDK_NGX_VULKAN_GetFeatureInstanceExtensionRequirements"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_GetFeatureRequirements(void) { return unsupported("NVSDK_NGX_VULKAN_GetFeatureRequirements"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_GetScratchBufferSize(void) { return unsupported("NVSDK_NGX_VULKAN_GetScratchBufferSize"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_Init(void) { return unsupported("NVSDK_NGX_VULKAN_Init"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_Init_Ext(void) { return unsupported("NVSDK_NGX_VULKAN_Init_Ext"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_Init_Ext2(void) { return unsupported("NVSDK_NGX_VULKAN_Init_Ext2"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_PopulateParameters_Impl(void) { return unsupported("NVSDK_NGX_VULKAN_PopulateParameters_Impl"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_ReleaseFeature(void) { return unsupported("NVSDK_NGX_VULKAN_ReleaseFeature"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_Shutdown(void) { return unsupported("NVSDK_NGX_VULKAN_Shutdown"); }
NGX_RESULT __cdecl NVSDK_NGX_VULKAN_Shutdown1(void) { return unsupported("NVSDK_NGX_VULKAN_Shutdown1"); }

/* ---------------------------------------------------------------- */
/* DllMain                                                           */
/* ---------------------------------------------------------------- */

BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, LPVOID reserved) {
    (void)h; (void)reserved;
    if (reason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(h);
        build_log_path();
        log_line("---- nvngx_dlssnr proxy attach ----");
        load_impl();
        /* Install the GMFW IAT hook before any NGX call goes through,
           so the snippet's identity check during the very first
           Init sees the spoofed nvngx.dll path. Lazy install (called
           from each wrapper) races with the first Init on DX12
           titles like Crimson Desert: Init returns BAD00007
           (NotInitialized) once, then succeeds on the second call.
        */
        install_gmfw_iat();
        g_veh = AddVectoredExceptionHandler(1, rs_veh);
    } else if (reason == DLL_PROCESS_DETACH) {
        log_line("---- nvngx_dlssnr proxy detach ----");
        if (g_veh) {
            RemoveVectoredExceptionHandler(g_veh);
            g_veh = NULL;
        }
    }
    return TRUE;
}
