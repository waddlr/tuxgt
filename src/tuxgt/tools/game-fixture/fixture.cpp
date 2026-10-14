// tuxgt-fixture-d3d11: deterministic D3D11 scene for the game-e2e-visual lane.
//
// Renders four colored quadrants as solid quads (one tiny shader pair,
// no assets, no RNG) so region color statistics are stable across runs.
// (D3D11 render-target clears ignore scissor rects, so region fills must
// be drawn, not cleared; proven during bring-up.) The region map and
// sampled assertion rects are documented in README.md beside this file;
// keep them in sync.
//
// Usage: tuxgt-fixture-d3d11.exe [--animate] [--readback] [--seconds N]
//   --readback   at frame 90, copy the backbuffer to staging and log
//                quadrant-center pixels (render-vs-present diagnosis).
//   --animate    rotate quadrant colors every 15 frames and sweep the
//                center marker horizontally (freeze/flicker oracles).
//   --seconds N  exit cleanly (code 0) after N seconds; default: run
//                until the window is closed / process is terminated.
// Progress is logged to fixture.log beside the exe (first-frame is the
// oracle anchor; console subsystem windows are avoided on purpose).

#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <stdio.h>
#include <string.h>

static const int kWidth = 640;
static const int kHeight = 360;
static const int kRotateEvery = 15;

// B8G8R8A8 clear colors as RGBA floats (channel order is R,G,B,A for Clear).
struct Color { float r, g, b; };
static const Color kBackground = {32.0f / 255.0f, 32.0f / 255.0f, 32.0f / 255.0f};
static const Color kQuadrants[4] = {
    {200.0f / 255.0f, 32.0f / 255.0f, 32.0f / 255.0f},   // TL red
    {32.0f / 255.0f, 200.0f / 255.0f, 32.0f / 255.0f},   // TR green
    {32.0f / 255.0f, 32.0f / 255.0f, 200.0f / 255.0f},   // BL blue
    {200.0f / 255.0f, 200.0f / 255.0f, 32.0f / 255.0f},  // BR yellow
};
static const Color kMarker = {240.0f / 255.0f, 240.0f / 255.0f, 240.0f / 255.0f};

static FILE *g_log = NULL;

static void logf_(const char *fmt, ...) {
    char buf[512];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    OutputDebugStringA(buf);
    if (g_log) {
        fputs(buf, g_log);
        fflush(g_log);
    }
}

static void open_log() {
    char path[MAX_PATH];
    DWORD n = GetModuleFileNameA(NULL, path, MAX_PATH);
    if (n == 0 || n >= MAX_PATH) return;
    char *slash = strrchr(path, '\\');
    if (slash) *(slash + 1) = '\0'; else path[0] = '\0';
    strncat(path, "fixture.log", MAX_PATH - strlen(path) - 1);
    g_log = fopen(path, "w");
}

static LRESULT CALLBACK wnd_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    if (msg == WM_CLOSE || msg == WM_DESTROY) {
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProcA(hwnd, msg, wp, lp);
}

struct Args {
    bool animate;
    bool readback;
    int seconds;  // 0 = forever
};

static Args parse_args() {
    Args a = {false, false, 0};
    char *cmd = GetCommandLineA();
    if (strstr(cmd, "--animate")) a.animate = true;
    if (strstr(cmd, "--readback")) a.readback = true;
    const char *s = strstr(cmd, "--seconds");
    if (s) a.seconds = atoi(s + strlen("--seconds"));
    return a;
}

// Self-check: copy the backbuffer to staging and log the center pixel of
// each quadrant (bytes are B,G,R,A). Distinguishes "fixture renders
// wrong" from "compositor never shows the window" during lane bring-up.
static void readback(ID3D11Device *device, ID3D11DeviceContext *ctx,
                     IDXGISwapChain *swapchain) {
    ID3D11Texture2D *bb = NULL;
    if (FAILED(swapchain->GetBuffer(0, __uuidof(ID3D11Texture2D),
                                    (void **)&bb)) || !bb) {
        logf_("fixture readback: GetBuffer failed\n");
        return;
    }
    D3D11_TEXTURE2D_DESC desc;
    bb->GetDesc(&desc);
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    desc.MiscFlags = 0;
    ID3D11Texture2D *staging = NULL;
    if (FAILED(device->CreateTexture2D(&desc, NULL, &staging)) || !staging) {
        logf_("fixture readback: staging create failed\n");
        bb->Release();
        return;
    }
    ctx->CopyResource(staging, bb);
    D3D11_MAPPED_SUBRESOURCE map;
    if (SUCCEEDED(ctx->Map(staging, 0, D3D11_MAP_READ, 0, &map))) {
        const BYTE *base = (const BYTE *)map.pData;
        const int pts[5][2] = {{160, 90}, {480, 90}, {160, 270},
                               {480, 270}, {320, 180}};
        const char *names[5] = {"TL", "TR", "BL", "BR", "C"};
        for (int i = 0; i < 5; i++) {
            const BYTE *p = base + pts[i][1] * (int)map.RowPitch +
                            pts[i][0] * 4;
            logf_("fixture readback %s bgra=%u,%u,%u,%u\n", names[i],
                  p[0], p[1], p[2], p[3]);
        }
        ctx->Unmap(staging, 0);
    } else {
        logf_("fixture readback: Map failed\n");
    }
    staging->Release();
    bb->Release();
}

// Solid-quad renderer: one dynamic vertex buffer + color constant
// buffer. Shaders are compiled at startup via the d3dcompiler_47.dll
// every Proton/Wine prefix ships (loaded dynamically; no import-lib or
// fxc dependency at build time).
static const char *kShaderSrc =
    "cbuffer C : register(b0) { float4 g_color; }\n"
    "float4 vs_main(float2 pos : POSITION) : SV_POSITION {\n"
    "    return float4(pos, 0.0, 1.0);\n"
    "}\n"
    "float4 ps_main() : SV_TARGET { return g_color; }\n";

typedef HRESULT(WINAPI *PFN_D3DCompile)(const void *, SIZE_T, const char *,
                                        const void *, void *, const char *,
                                        const char *, UINT, UINT, ID3DBlob **,
                                        ID3DBlob **);

struct Quads {
    ID3D11VertexShader *vs;
    ID3D11PixelShader *ps;
    ID3D11InputLayout *layout;
    ID3D11Buffer *vb;
    ID3D11Buffer *cb;
};

static bool quads_init(ID3D11Device *device, Quads *q) {
    *q = {};
    HMODULE dll = LoadLibraryA("d3dcompiler_47.dll");
    if (!dll) dll = LoadLibraryA("d3dcompiler_43.dll");
    if (!dll) {
        logf_("fixture fatal: no d3dcompiler dll\n");
        return false;
    }
    PFN_D3DCompile compile =
        (PFN_D3DCompile)GetProcAddress(dll, "D3DCompile");
    if (!compile) {
        logf_("fixture fatal: D3DCompile missing\n");
        return false;
    }
    ID3DBlob *vs_blob = NULL, *ps_blob = NULL, *err = NULL;
    HRESULT hr = compile(kShaderSrc, strlen(kShaderSrc), NULL, NULL, NULL,
                         "vs_main", "vs_4_0", 0, 0, &vs_blob, &err);
    if (SUCCEEDED(hr))
        hr = compile(kShaderSrc, strlen(kShaderSrc), NULL, NULL, NULL,
                     "ps_main", "ps_4_0", 0, 0, &ps_blob, &err);
    if (FAILED(hr) || !vs_blob || !ps_blob) {
        logf_("fixture fatal: shader compile hr=0x%lx %s\n",
              (unsigned long)hr,
              err ? (const char *)err->GetBufferPointer() : "");
        return false;
    }
    device->CreateVertexShader(vs_blob->GetBufferPointer(),
                               vs_blob->GetBufferSize(), NULL, &q->vs);
    device->CreatePixelShader(ps_blob->GetBufferPointer(),
                              ps_blob->GetBufferSize(), NULL, &q->ps);
    D3D11_INPUT_ELEMENT_DESC elem = {"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT,
                                     0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0};
    device->CreateInputLayout(&elem, 1, vs_blob->GetBufferPointer(),
                              vs_blob->GetBufferSize(), &q->layout);
    vs_blob->Release();
    ps_blob->Release();

    D3D11_BUFFER_DESC bd = {};
    bd.Usage = D3D11_USAGE_DYNAMIC;
    bd.CPUAccessFlags = D3D11_CPU_ACCESS_WRITE;
    bd.BindFlags = D3D11_BIND_VERTEX_BUFFER;
    bd.ByteWidth = 6 * 2 * sizeof(float);
    device->CreateBuffer(&bd, NULL, &q->vb);
    bd.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
    bd.ByteWidth = 16;
    device->CreateBuffer(&bd, NULL, &q->cb);
    if (!q->vs || !q->ps || !q->layout || !q->vb || !q->cb) {
        logf_("fixture fatal: quad renderer create failed\n");
        return false;
    }
    return true;
}

static void quads_release(Quads *q) {
    if (q->vs) q->vs->Release();
    if (q->ps) q->ps->Release();
    if (q->layout) q->layout->Release();
    if (q->vb) q->vb->Release();
    if (q->cb) q->cb->Release();
}

// Draw one solid rect in pixel coordinates over the bound render target.
static void draw_rect(ID3D11DeviceContext *ctx, Quads *q, LONG left,
                      LONG top, LONG right, LONG bottom, const Color &c) {
    float rgba[4] = {c.r, c.g, c.b, 1.0f};
    D3D11_MAPPED_SUBRESOURCE map;
    if (SUCCEEDED(ctx->Map(q->cb, 0, D3D11_MAP_WRITE_DISCARD, 0, &map))) {
        memcpy(map.pData, rgba, sizeof(rgba));
        ctx->Unmap(q->cb, 0);
    }
    // Pixel -> NDC (pixel top is +Y in NDC).
    float x0 = (float)left / kWidth * 2.0f - 1.0f;
    float x1 = (float)right / kWidth * 2.0f - 1.0f;
    float y0 = 1.0f - (float)top / kHeight * 2.0f;
    float y1 = 1.0f - (float)bottom / kHeight * 2.0f;
    float verts[12] = {x0, y0, x1, y0, x0, y1, x1, y0, x1, y1, x0, y1};
    if (SUCCEEDED(ctx->Map(q->vb, 0, D3D11_MAP_WRITE_DISCARD, 0, &map))) {
        memcpy(map.pData, verts, sizeof(verts));
        ctx->Unmap(q->vb, 0);
    }
    UINT stride = 2 * sizeof(float), offset = 0;
    ctx->IASetVertexBuffers(0, 1, &q->vb, &stride, &offset);
    ctx->IASetInputLayout(q->layout);
    ctx->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    ctx->VSSetShader(q->vs, NULL, 0);
    ctx->PSSetShader(q->ps, NULL, 0);
    ctx->PSSetConstantBuffers(0, 1, &q->cb);
    ctx->Draw(6, 0);
}

int WINAPI WinMain(HINSTANCE inst, HINSTANCE, LPSTR, int) {
    open_log();
    Args args = parse_args();
    logf_("fixture start animate=%d seconds=%d\n", args.animate ? 1 : 0,
          args.seconds);

    WNDCLASSA wc = {};
    wc.lpfnWndProc = wnd_proc;
    wc.hInstance = inst;
    wc.lpszClassName = "TuxGtFixtureD3D11";
    RegisterClassA(&wc);

    RECT wr = {0, 0, kWidth, kHeight};
    AdjustWindowRect(&wr, WS_OVERLAPPEDWINDOW, FALSE);
    HWND hwnd = CreateWindowExA(0, wc.lpszClassName, "TuxGT Fixture D3D11",
                                WS_OVERLAPPEDWINDOW, CW_USEDEFAULT,
                                CW_USEDEFAULT, wr.right - wr.left,
                                wr.bottom - wr.top, NULL, NULL, inst, NULL);
    if (!hwnd) {
        logf_("fixture fatal: CreateWindow failed\n");
        return 1;
    }
    ShowWindow(hwnd, SW_SHOW);

    DXGI_SWAP_CHAIN_DESC sd = {};
    sd.BufferDesc.Width = kWidth;
    sd.BufferDesc.Height = kHeight;
    sd.BufferDesc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
    sd.SampleDesc.Count = 1;
    sd.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    sd.BufferCount = 2;
    sd.OutputWindow = hwnd;
    sd.Windowed = TRUE;
    sd.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;

    ID3D11Device *device = NULL;
    ID3D11DeviceContext *ctx = NULL;
    IDXGISwapChain *swapchain = NULL;
    D3D_FEATURE_LEVEL level = D3D_FEATURE_LEVEL_11_0;
    const D3D_FEATURE_LEVEL want[] = {D3D_FEATURE_LEVEL_11_0,
                                      D3D_FEATURE_LEVEL_10_0};
    HRESULT hr = D3D11CreateDeviceAndSwapChain(
        NULL, D3D_DRIVER_TYPE_HARDWARE, NULL, 0, want, 2,
        D3D11_SDK_VERSION, &sd, &swapchain, &device, &level, &ctx);
    if (FAILED(hr) || !device || !swapchain) {
        logf_("fixture fatal: D3D11CreateDeviceAndSwapChain hr=0x%lx\n",
              (unsigned long)hr);
        return 2;
    }
    logf_("fixture device feature_level=0x%x\n", (unsigned)level);

    ID3D11Texture2D *backbuffer = NULL;
    hr = swapchain->GetBuffer(0, __uuidof(ID3D11Texture2D),
                              (void **)&backbuffer);
    ID3D11RenderTargetView *rtv = NULL;
    if (SUCCEEDED(hr))
        hr = device->CreateRenderTargetView(backbuffer, NULL, &rtv);
    if (backbuffer) backbuffer->Release();
    if (FAILED(hr) || !rtv) {
        logf_("fixture fatal: render target hr=0x%lx\n", (unsigned long)hr);
        return 3;
    }

    D3D11_RASTERIZER_DESC rd = {};
    rd.FillMode = D3D11_FILL_SOLID;
    rd.CullMode = D3D11_CULL_NONE;
    rd.ScissorEnable = FALSE;
    ID3D11RasterizerState *rs = NULL;
    device->CreateRasterizerState(&rd, &rs);
    ctx->RSSetState(rs);

    Quads quads;
    if (!quads_init(device, &quads)) return 4;

    D3D11_VIEWPORT vp = {};
    vp.Width = (FLOAT)kWidth;
    vp.Height = (FLOAT)kHeight;
    vp.MaxDepth = 1.0f;
    ctx->RSSetViewports(1, &vp);
    ctx->OMSetRenderTargets(1, &rtv, NULL);

    const int hw = kWidth / 2, hh = kHeight / 2;
    const LONG quad_rects[4][4] = {
        {0, 0, hw, hh}, {hw, 0, kWidth, hh},
        {0, hh, hw, kHeight}, {hw, hh, kWidth, kHeight},
    };

    ULONGLONG t0 = GetTickCount64();
    unsigned long long frame = 0;
    bool quit = false;
    while (!quit) {
        MSG msg;
        while (PeekMessageA(&msg, NULL, 0, 0, PM_REMOVE)) {
            if (msg.message == WM_QUIT) quit = true;
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
        if (quit) break;

        int rot = args.animate ? (int)((frame / kRotateEvery) % 4) : 0;
        float bg[4] = {kBackground.r, kBackground.g, kBackground.b, 1.0f};
        ctx->OMSetRenderTargets(1, &rtv, NULL);
        ctx->ClearRenderTargetView(rtv, bg);
        for (int q = 0; q < 4; q++) {
            draw_rect(ctx, &quads, quad_rects[q][0], quad_rects[q][1],
                      quad_rects[q][2], quad_rects[q][3],
                      kQuadrants[(q + rot) % 4]);
        }
        // Center marker 16x16; sweeps horizontally in --animate mode.
        LONG mx = hw, my = hh;
        if (args.animate) mx = hw + (LONG)((frame * 2) % 240) - 120;
        draw_rect(ctx, &quads, mx - 8, my - 8, mx + 8, my + 8, kMarker);

        if (args.readback && frame == 90)
            readback(device, ctx, swapchain);
        swapchain->Present(1, 0);
        frame++;
        if (frame == 1) logf_("fixture first-frame\n");
        if (frame % 300 == 0)
            logf_("fixture frames=%llu\n", frame);

        if (args.seconds > 0 &&
            GetTickCount64() - t0 >= (ULONGLONG)args.seconds * 1000) {
            logf_("fixture exit seconds=%d frames=%llu\n", args.seconds,
                  frame);
            quit = true;
        }
    }

    logf_("fixture done frames=%llu\n", frame);
    quads_release(&quads);
    if (rs) rs->Release();
    if (rtv) rtv->Release();
    if (swapchain) swapchain->Release();
    if (ctx) ctx->Release();
    if (device) device->Release();
    if (g_log) fclose(g_log);
    return 0;
}
