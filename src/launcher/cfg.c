#define _GNU_SOURCE
#include "tuxgt-launcher.h"

void resolve_cfg(const char *stem, struct game_cfg *c) {
    memset(c, 0, sizeof(*c));
    char init[PATH_MAX];
    if (!find_init_ini(init, sizeof(init))) return;
    if (!ini_has_sect(init, stem)) return; // opt-in: no section, no ReShade

    char type[64] = {0};
    if (!ini_get(init, stem, "Type", type, sizeof(type)))
        ini_get(init, "Defaults", "Type", type, sizeof(type));
    if (!type[0]) snprintf(type, sizeof(type), "dx12_64");
    snprintf(c->type, sizeof(c->type), "%s", type);

    const char *env_d = getenv("TUXGT_GAME_DIR");
    if (env_d && env_d[0]) {
        snprintf(c->home_unix, sizeof(c->home_unix), "%s", env_d);
    } else {
        char games[PATH_MAX] = {0};
        const char *env_g = getenv("TUXGT_GAMES");
        if (env_g && env_g[0]) {
            snprintf(games, sizeof(games), "%s", env_g);
        } else {
            char spec[PATH_MAX] = {0};
            ini_get(init, "Init", "GamesDir", spec, sizeof(spec));
            if (spec[0]) resolve_from_ini(init, spec, games, sizeof(games));
            else default_games_dir(games, sizeof(games));
        }
        snprintf(c->home_unix, sizeof(c->home_unix), "%s/%s", games, stem);
    }

    ini_get_list(init, stem, "LoadDLL", c->loaddll, sizeof(c->loaddll));
    ini_get_list(init, stem, "IncludeFile", c->includes, sizeof(c->includes));

    const char *env_a = getenv("TUXGT_DEPOT");
    if (env_a && env_a[0]) {
        snprintf(c->depot_unix, sizeof(c->depot_unix), "%s", env_a);
    } else {
        char spec[PATH_MAX] = {0};
        ini_get(init, "Init", "DepotDir", spec, sizeof(spec));
        if (spec[0]) {
            resolve_from_ini(init, spec, c->depot_unix, sizeof(c->depot_unix));
        } else {
            char dir[PATH_MAX];
            snprintf(dir, sizeof(dir), "%s", init);
            char *slash = strrchr(dir, '/');
            if (slash) *slash = 0;
            else snprintf(dir, sizeof(dir), ".");
            snprintf(c->depot_unix, sizeof(c->depot_unix), "%s/mods", dir);
        }
    }
    c->ok = 1;
    char msg[PATH_MAX + 128];
    snprintf(msg, sizeof(msg), "cfg stem=%s home=%s loaddll=%zu includes=%zu", stem,
             c->home_unix, strlen(c->loaddll), strlen(c->includes));
    log_debug(msg);
}
