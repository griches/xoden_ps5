// SPDX-License-Identifier: GPL-3.0-or-later
// One console job: wait for Playden to exit, run its Xbox worker, then return.
// Never terminates a title and abandons the handoff if another game is started.
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/file.h>
#include <sys/stat.h>

#define ROOT "/data/playden/xbox/"
typedef struct { uint32_t size, user_id, app_opt, pad; uint64_t crash_report, check_flag; } launch_params;
int sceUserServiceInitialize(void*);
int sceUserServiceGetForegroundUser(uint32_t*);
int sceLncUtilInitialize(void);
uint32_t sceLncUtilGetAppIdOfRunningBigApp(void);
int sceLncUtilGetAppTitleId(uint32_t, char*);
uint32_t sceLncUtilLaunchApp(const char*, const char**, launch_params*);
uint32_t sceSystemServiceLaunchApp(const char*, const char**, launch_params*);

static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec + t.tv_nsec / 1e9; }
static int line(FILE* file, char* value, size_t size) {
    if (!fgets(value, size, file) || !strchr(value, '\n')) return 0;
    value[strcspn(value, "\r\n")] = 0; return 1;
}
static int uuid(const char* value) {
    if (strlen(value) != 36) return 0;
    for (int i = 0; i < 36; ++i) {
        if (i == 8 || i == 13 || i == 18 || i == 23) { if (value[i] != '-') return 0; }
        else if (!((value[i] >= '0' && value[i] <= '9') || (value[i] >= 'a' && value[i] <= 'f'))) return 0;
    }
    return 1;
}
static int worker_title(const char* title) {
    if (strlen(title) != 9 || strncmp(title, "FAKE78", 6)) return 0;
    for (int i = 6; i < 9; ++i) if (title[i] < '0' || title[i] > '9') return 0;
    return 1;
}
static int report(const char* id, const char* phase) {
    if (seteuid(1)) return 0;
    int fd = open(ROOT "handoff-status.tmp", O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) { seteuid(0); return 0; }
    char text[128]; int n = snprintf(text, sizeof text, "%s %s\n", id, phase);
    int ok = fchmod(fd, 0600) == 0 && write(fd, text, n) == n && fsync(fd) == 0;
    if (close(fd)) ok = 0;
    if (ok && rename(ROOT "handoff-status.tmp", ROOT "handoff-status.txt")) ok = 0;
    seteuid(0); return ok;
}
// 0 idle, 1 a known title, -1 unknown (never permission to launch).
static int current(char* title) {
    title[0] = 0;
    uint32_t app = sceLncUtilGetAppIdOfRunningBigApp();
    if (!app || app == UINT32_MAX) return 0;
    char buffer[32] = {0};
    if (sceLncUtilGetAppTitleId(app, buffer) || strlen(buffer) != 9) return -1;
    memcpy(title, buffer, 10); return 1;
}
static int launch(const char* title) {
    uint32_t user = 0;
    if (sceUserServiceGetForegroundUser(&user)) return 0;
    launch_params params = {sizeof params, user, 0, 0, 0, 0};
    uint32_t rc = sceLncUtilLaunchApp(title, NULL, &params);
    if (rc & 0x80000000u) { params.check_flag = 1; rc = sceSystemServiceLaunchApp(title, NULL, &params); }
    return !(rc & 0x80000000u);
}
int main(void) {
    if (seteuid(1)) return 1;
    int lock = open(ROOT "handoff.lock", O_CREAT | O_WRONLY, 0600);
    if (lock < 0 || flock(lock, LOCK_EX | LOCK_NB)) { if (lock >= 0) close(lock); return 1; }
    char id[64], worker[32], home[32];
    FILE* file = fopen(ROOT "handoff-request.txt", "r");
    int valid = file && line(file, id, sizeof id) && line(file, worker, sizeof worker) && line(file, home, sizeof home)
        && fgetc(file) == EOF && uuid(id) && worker_title(worker)
        && (!strcmp(home, "FAKE77031") || !strcmp(home, "PPSA97031"));
    if (file) fclose(file);
    if (!valid || seteuid(0)) { close(lock); return 1; }
    sceUserServiceInitialize(NULL); sceLncUtilInitialize();
    char title[32]; int state = current(title);
    if (state < 0 || (state && strcmp(title, home))) { report(id, "busy"); close(lock); return 1; }
    if (!report(id, "armed")) { close(lock); return 1; }
    double start = now();
    while ((state = current(title)) == 1 && !strcmp(title, home) && now() - start < 90) usleep(250000);
    if (state != 0) { report(id, "canceled"); close(lock); return 1; }
    if (!launch(worker)) { report(id, "launch-failed"); close(lock); return 1; }
    report(id, "starting"); start = now(); int seen = 0;
    while (now() - start < 24 * 60 * 60) {
        state = current(title);
        if (state < 0 || (state && strcmp(title, worker))) { report(id, "interrupted"); close(lock); return 1; }
        if (state) seen = 1;
        if (!state && (seen || now() - start > 30)) break;
        usleep(250000);
    }
    if (state != 0) { report(id, "timeout"); close(lock); return 1; }
    // Let the console finish shutting down Wine. Recheck before opening Playden.
    usleep(1000000);
    if (current(title) != 0) { report(id, "interrupted"); close(lock); return 1; }
    int ok = launch(home); report(id, ok ? "returned" : "return-failed"); close(lock); return ok ? 0 : 1;
}
