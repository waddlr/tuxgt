// Persistent virtual pointer + keyboard for a headless GUI lane.
//
// sway's `seat … cursor` commands move the cursor but deliver nothing to
// Wayland clients when the seat has no input devices (caps=0: the client never
// binds wl_pointer/wl_keyboard). This daemon holds a real virtual pointer and
// a real virtual keyboard for the lane's lifetime, so the seat has both
// capabilities before the app starts and every motion/button/axis/key event
// reaches the client. The keyboard only holds the device (with a us keymap);
// actual typing goes through wtype.
//
// Build: gcc on demand (see src/tuxgt/tools/gui-session). Needs the vendored protocol
// XMLs (wlr-virtual-pointer-unstable-v1.xml from the wlroots protocol set,
// virtual-keyboard-unstable-v1.xml from wayland-protocols-misc) and
// wayland-scanner, plus libwayland-client and libxkbcommon dev files.
// No runtime deps beyond those two shared libraries.
//
// Stdin protocol (one command per line):
//   M x y     absolute motion to output coords (extents = argv W H)
//   D btn     button down (1,2,3,8,9)
//   U btn     button up
//   C btn     click (down+up)
//   S dx dy   scroll notches (horizontal, vertical)
//   Q         quit
// Prints "ready" on stdout once the devices exist.
#define _GNU_SOURCE

#include <fcntl.h>
#include <linux/input-event-codes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include <wayland-util.h>
#include <xkbcommon/xkbcommon.h>

#include "wlr-virtual-pointer-unstable-v1-client-protocol.h"
#include "virtual-keyboard-unstable-v1-client-protocol.h"

static struct wl_display *display;
static struct wl_seat *seat;
static struct zwlr_virtual_pointer_manager_v1 *vp_manager;
static struct zwlr_virtual_pointer_v1 *vp;
static struct zwp_virtual_keyboard_manager_v1 *vk_manager;
static struct zwp_virtual_keyboard_v1 *vkbd;
static uint32_t ext_w = 1600, ext_h = 1000;

static uint32_t now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}

static void registry_global(void *data, struct wl_registry *reg, uint32_t name,
                            const char *interface, uint32_t version) {
    (void)data;
    if (strcmp(interface, wl_seat_interface.name) == 0) {
        uint32_t v = version < 9 ? version : 9;
        seat = wl_registry_bind(reg, name, &wl_seat_interface, v);
    } else if (strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name) == 0) {
        uint32_t v = version < 2 ? version : 2;
        vp_manager = wl_registry_bind(reg, name, &zwlr_virtual_pointer_manager_v1_interface, v);
    } else if (strcmp(interface, zwp_virtual_keyboard_manager_v1_interface.name) == 0) {
        vk_manager = wl_registry_bind(reg, name, &zwp_virtual_keyboard_manager_v1_interface, 1);
    }
}

static void registry_global_remove(void *data, struct wl_registry *reg, uint32_t name) {
    (void)data;
    (void)reg;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {
    .global = registry_global,
    .global_remove = registry_global_remove,
};

static uint32_t map_button(long b) {
    switch (b) {
    case 1: return BTN_LEFT;
    case 2: return BTN_MIDDLE;
    case 3: return BTN_RIGHT;
    case 8: return BTN_SIDE;
    case 9: return BTN_EXTRA;
    default: return 0;
    }
}

static void send_scroll(long dx, long dy) {
    uint32_t t = now_ms();
    zwlr_virtual_pointer_v1_axis_source(vp, WL_POINTER_AXIS_SOURCE_WHEEL);
    if (dy) {
        zwlr_virtual_pointer_v1_axis(vp, t, WL_POINTER_AXIS_VERTICAL_SCROLL,
                                     wl_fixed_from_double((double)dy * 10.0));
        zwlr_virtual_pointer_v1_axis_discrete(vp, t, WL_POINTER_AXIS_VERTICAL_SCROLL,
                                              (double)dy * 10.0, (int32_t)dy);
    }
    if (dx) {
        zwlr_virtual_pointer_v1_axis(vp, t, WL_POINTER_AXIS_HORIZONTAL_SCROLL,
                                     wl_fixed_from_double((double)dx * 10.0));
        zwlr_virtual_pointer_v1_axis_discrete(vp, t, WL_POINTER_AXIS_HORIZONTAL_SCROLL,
                                              (double)dx * 10.0, (int32_t)dx);
    }
    zwlr_virtual_pointer_v1_frame(vp);
}

int main(int argc, char **argv) {
    if (argc >= 4) {
        ext_w = (uint32_t)strtoul(argv[2], NULL, 10);
        ext_h = (uint32_t)strtoul(argv[3], NULL, 10);
    }
    if (argc >= 2 && getenv("WAYLAND_DISPLAY") == NULL)
        setenv("WAYLAND_DISPLAY", argv[1], 1);

    display = wl_display_connect(NULL);
    if (!display) {
        fprintf(stderr, "wlinput: cannot connect to wayland display\n");
        return 1;
    }
    struct wl_registry *reg = wl_display_get_registry(display);
    wl_registry_add_listener(reg, &registry_listener, NULL);
    wl_display_roundtrip(display);
    if (!seat || !vp_manager) {
        fprintf(stderr, "wlinput: seat or virtual-pointer manager missing\n");
        return 1;
    }
    vp = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(vp_manager, seat);
    // A persistent keyboard keeps seat keyboard capability up from lane start,
    // so the app binds wl_keyboard at startup and wtype keys land. The daemon
    // never sends keys itself; it only holds the device.
    if (vk_manager) {
        vkbd = zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(vk_manager, seat);
        struct xkb_context *xkb = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
        if (xkb) {
            struct xkb_rule_names names = {
                .rules = "evdev", .model = "pc105", .layout = "us",
                .variant = NULL, .options = NULL,
            };
            struct xkb_keymap *map =
                xkb_keymap_new_from_names(xkb, &names, XKB_KEYMAP_COMPILE_NO_FLAGS);
            if (map) {
                char *str = xkb_keymap_get_as_string(map, XKB_KEYMAP_FORMAT_TEXT_V1);
                if (str) {
                    size_t len = strlen(str) + 1;
                    int fd = memfd_create("xkb-keymap", MFD_CLOEXEC);
                    if (fd >= 0) {
                        if (ftruncate(fd, (off_t)len) == 0 &&
                            write(fd, str, len) == (ssize_t)len) {
                            zwp_virtual_keyboard_v1_keymap(
                                vkbd, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1, fd, (uint32_t)len);
                        } else {
                            fprintf(stderr, "wlinput: keymap fd write failed\n");
                        }
                        close(fd);
                    } else {
                        fprintf(stderr, "wlinput: memfd_create failed\n");
                    }
                    free(str);
                }
                xkb_keymap_unref(map);
            } else {
                fprintf(stderr, "wlinput: xkb keymap compile failed\n");
            }
            xkb_context_unref(xkb);
        }
    } else {
        fprintf(stderr, "wlinput: virtual-keyboard manager missing (keys via wtype may race)\n");
    }
    wl_display_roundtrip(display);

    printf("ready\n");
    fflush(stdout);

    char line[256];
    for (;;) {
        if (!fgets(line, sizeof(line), stdin)) {
            if (feof(stdin)) {
                // Writers come and go; the read end stays open. Wait for more.
                clearerr(stdin);
                struct timespec rq = { 0, 50000000 };
                nanosleep(&rq, NULL);
                continue;
            }
            break;
        }
        char op = line[0];
        long a = 0, b = 0;
        sscanf(line + 1, "%ld %ld", &a, &b);
        uint32_t t = now_ms();
        switch (op) {
        case 'M':
            zwlr_virtual_pointer_v1_motion_absolute(vp, t, (uint32_t)a, (uint32_t)b, ext_w, ext_h);
            zwlr_virtual_pointer_v1_frame(vp);
            break;
        case 'D': {
            uint32_t code = map_button(a);
            if (code)
                zwlr_virtual_pointer_v1_button(vp, t, code,
                                               WL_POINTER_BUTTON_STATE_PRESSED);
            zwlr_virtual_pointer_v1_frame(vp);
            break;
        }
        case 'U': {
            uint32_t code = map_button(a);
            if (code)
                zwlr_virtual_pointer_v1_button(vp, t, code,
                                               WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(vp);
            break;
        }
        case 'C': {
            uint32_t code = map_button(a);
            if (code) {
                zwlr_virtual_pointer_v1_button(vp, t, code,
                                               WL_POINTER_BUTTON_STATE_PRESSED);
                zwlr_virtual_pointer_v1_button(vp, now_ms(), code,
                                               WL_POINTER_BUTTON_STATE_RELEASED);
            }
            zwlr_virtual_pointer_v1_frame(vp);
            break;
        }
        case 'S':
            send_scroll(a, b);
            break;
        case 'Q':
            goto done;
        default:
            fprintf(stderr, "wlinput: bad command: %s", line);
            continue;
        }
        wl_display_flush(display);
    }
done:
    return 0;
}
