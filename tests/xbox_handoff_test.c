// SPDX-License-Identifier: GPL-3.0-or-later
// Exercise the actual payload lifecycle with fake console APIs and real private
// request/status/lock files. Host tests never change their effective uid.
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <limits.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
static int fake_seteuid(uid_t uid);
static int fake_usleep(useconds_t microseconds);
static int fake_clock_gettime(clockid_t clock, struct timespec* value);
#define seteuid fake_seteuid
#define usleep fake_usleep
#define clock_gettime fake_clock_gettime
#define ROOT ""
#define main payload_main
#include "../tools/xbox-handoff/main.c"
#undef main
#undef seteuid
#undef usleep
#undef clock_gettime

static int launching, launches, worker_polls, home_running, busy, unknown, refuse;
static double fake_time;
static char targets[2][16], argument[64];
static int fake_seteuid(uid_t uid) { (void)uid; return 0; }
static int fake_usleep(useconds_t microseconds) {
    fake_time += microseconds / 1000000.0;
    home_running = 0;
    return 0;
}
static int fake_clock_gettime(clockid_t clock, struct timespec* value) {
    (void)clock;
    value->tv_sec = (time_t)fake_time;
    value->tv_nsec = (long)((fake_time - value->tv_sec) * 1000000000);
    return 0;
}
int sceUserServiceInitialize(void* unused) { (void)unused; return 0; }
int sceLncUtilInitialize(void) { return 0; }
int sceUserServiceGetForegroundUser(uint32_t* user) { *user = 1; return 0; }
uint32_t sceLncUtilGetAppIdOfRunningBigApp(void) {
    if (busy || unknown) return 5;
    if (home_running) return 1;
    return launching && worker_polls++ < 2 ? 2 : 0;
}
int sceLncUtilGetAppTitleId(uint32_t app, char* title) {
    if (unknown) return -1;
    strcpy(title, app == 1 ? "FAKE77031" : app == 2 ? "FAKE78002" : "OTHER0001");
    return 0;
}
uint32_t sceLncUtilLaunchApp(const char* title, const char** args, launch_params* params) {
    assert(params->user_id == 1);
    if (refuse) return 0x80000001u;
    assert(launches < 2);
    strcpy(targets[launches++], title);
    if (args) strcpy(argument, args[0]);
    launching = 1;
    return 0;
}
uint32_t sceSystemServiceLaunchApp(const char* title, const char** args, launch_params* params) {
    return sceLncUtilLaunchApp(title, args, params);
}
static void request(const char* mode) {
    FILE* file = fopen("handoff-request.txt", "w"); assert(file);
    fprintf(file, "11111111-1111-4111-8111-111111111111\nFAKE78002\nFAKE77031\n%s", mode);
    assert(fclose(file) == 0);
    launching = launches = worker_polls = busy = unknown = refuse = 0;
    home_running = 1; fake_time = 0; argument[0] = 0;
    unlink("handoff-status.txt");
}
static void status(const char* phase) {
    char result[128], expected[128];
    FILE* file = fopen("handoff-status.txt", "r"); assert(file);
    assert(fgets(result,sizeof result,file)); fclose(file);
    snprintf(expected,sizeof expected,"11111111-1111-4111-8111-111111111111 %s\n",phase);
    assert(!strcmp(result,expected));
}
int main(void) {
    char previous[PATH_MAX], folder[] = "/tmp/playden-handoff-XXXXXX";
    assert(getcwd(previous,sizeof previous) && mkdtemp(folder));
    assert(chdir(folder) == 0);
    request("game-session\n");
    assert(payload_main() == 0 && launches == 1 && worker_polls == 0);
    assert(!strcmp(targets[0], "FAKE78002"));
    assert(!strcmp(argument, "session=11111111-1111-4111-8111-111111111111"));
    status("launched");
    request("");
    assert(payload_main() == 0 && launches == 2 && !argument[0]);
    assert(!strcmp(targets[1], "FAKE77031")); status("returned");
    request("game-session\n"); busy = 1;
    assert(payload_main() == 1 && launches == 0); status("busy");
    request("game-session\n"); unknown = 1;
    assert(payload_main() == 1 && launches == 0); status("busy");
    request("game-session\n"); refuse = 1;
    assert(payload_main() == 1 && launches == 0); status("launch-failed");
    request("game-session\ntrailing\n");
    assert(payload_main() == 1 && launches == 0);
    assert(access("handoff-status.txt", F_OK) != 0);
    unlink("handoff.lock"); unlink("handoff-request.txt");
    assert(chdir(previous) == 0 && rmdir(folder) == 0);
    puts("ok"); return 0;
}
