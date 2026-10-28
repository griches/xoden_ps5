/* uidcp - run file commands on the PS5 as Playden's user (uid 1).
 *
 * Root does not override file permissions on the PS5, and Playden keeps its
 * bottles in /data/playden/games (uid 1, 0700), so FTP cannot reach them.
 * This payload reads /data/xodus-drop/uidcp.txt, one command per line with
 * tab-separated fields, runs it with effective uid 1 and writes
 * /data/xodus-drop/uidcp.log:
 *
 *   ls  <dir>
 *   cp  <src> <dst>    dst must be under /data/playden/; an existing dst is
 *                      kept once as <dst>.xodus-bak
 *   cp-executable <src> <dst> as cp, with executable mode 0755
 *   get <src> <name>   copies src to /data/xodus-drop/<name> (mode 0644)
 *   get-raw <src> <name>  as get, with a 13-byte XODUS-RAW-V1 newline prefix
 *   mkdir <dir>        dir must be under /data/playden/ (mode 0700)
 *   rm <path>          path must be under /data/playden/
 *   rm-xbox-partial <root> canonical Xbox content root; caller checks job state
 */
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define DROP "/data/xodus-drop"
#define ALLOWED "/data/playden/"

static FILE *out;
static int copy_file(const char *src, const char *dst, mode_t mode);

static void say(const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    vfprintf(out, fmt, ap);
    va_end(ap);
    fputc('\n', out);
    fflush(out);
}

static int copy_file_mode(const char *src, const char *dst, mode_t mode, int raw)
{
    char buf[1 << 16];
    ssize_t n;
    long long total = 0;
    int in = open(src, O_RDONLY);
    if (in < 0) {
        say("  open %s: %s", src, strerror(errno));
        return -1;
    }
    int fd = open(dst, O_WRONLY | O_CREAT | O_TRUNC, mode);
    if (fd < 0) {
        say("  create %s: %s", dst, strerror(errno));
        close(in);
        return -1;
    }
    // Some console FTP servers transparently decode SELF files on retrieval.
    // A non-executable prefix preserves the exact on-disk bytes for verification.
    if (raw && write(fd, "XODUS-RAW-V1\n", 13) != 13) {
        say("  prefix write failed"); close(in); close(fd); return -1;
    }
    while ((n = read(in, buf, sizeof buf)) > 0) {
        ssize_t offset = 0;
        while (offset < n) {
            ssize_t written = write(fd, buf + offset, n - offset);
            if (written < 0 && errno == EINTR) continue;
            if (written <= 0) {
                say("  write %s: %s", dst, strerror(errno));
                close(in);
                close(fd);
                return -1;
            }
            offset += written;
        }
        total += n;
    }
    int failed = n < 0;
    if (fchmod(fd, mode) != 0 || fsync(fd) != 0) failed = 1;
    close(in);
    if (close(fd) != 0) failed = 1;
    if (failed) {
        say("  copy failed: %s", strerror(errno));
        return -1;
    }
    say("  %lld bytes", total);
    return 0;
}

static void do_ls(const char *dir)
{
    DIR *d = opendir(dir);
    struct dirent *e;
    if (!d) {
        say("  opendir: %s", strerror(errno));
        return;
    }
    while ((e = readdir(d))) {
        char path[1024];
        struct stat st;
        if (!strcmp(e->d_name, ".") || !strcmp(e->d_name, ".."))
            continue;
        snprintf(path, sizeof path, "%s/%s", dir, e->d_name);
        if (lstat(path, &st) == 0)
            say("  %c %04o %u %10lld %s", S_ISDIR(st.st_mode) ? 'd' : '-', st.st_mode & 07777,
                st.st_uid, (long long)st.st_size, e->d_name);
        else
            say("  ? %s (%s)", e->d_name, strerror(errno));
    }
    closedir(d);
}

static void do_cp(const char *src, const char *dst, mode_t mode)
{
    char bak[1024], temp[1024];
    struct stat st;
    if (strncmp(dst, ALLOWED, strlen(ALLOWED)) || strstr(dst, "/../")) {
        say("  refused: destination outside " ALLOWED);
        return;
    }
    if (strlen(dst) > sizeof bak - 32) {
        say("  refused: destination too long");
        return;
    }
    snprintf(bak, sizeof bak, "%s.xodus-bak", dst);
    snprintf(temp, sizeof temp, "%s.xodus-new", dst);
    if (stat(dst, &st) == 0 && stat(bak, &st) != 0) {
        if (copy_file(dst, bak, mode) == 0)
            say("  kept old as %s", bak);
        else {
            say("  backup %s: %s", bak, strerror(errno));
            return;
        }
    }
    if (copy_file(src, temp, mode) == 0) {
        if (rename(temp, dst) != 0)
            say("  publish %s: %s", dst, strerror(errno));
        else
            say("  published %s", dst);
    } else {
        unlink(temp);
    }
}

static int allowed(const char *path)
{
    if (strncmp(path, ALLOWED, strlen(ALLOWED)) || strstr(path, "/../")) {
        say("  refused: path outside " ALLOWED);
        return 0;
    }
    return 1;
}

static void do_mkdir(const char *dir)
{
    if (!allowed(dir))
        return;
    if (mkdir(dir, 0700) == 0)
        say("  created");
    else
        say("  mkdir: %s", strerror(errno));
}

static void do_rm(const char *path)
{
    if (!allowed(path))
        return;
    if (unlink(path) == 0)
        say("  removed");
    else
        say("  unlink: %s", strerror(errno));
}

/* Only canonical Xbox content roots; never traverse a symlink. */
static int remove_tree(const char *path, long long *bytes)
{
    struct stat st;
    if (lstat(path, &st) != 0) return errno == ENOENT ? 0 : -1;
    if (!S_ISDIR(st.st_mode)) {
        if (unlink(path) != 0) return -1;
        if (S_ISREG(st.st_mode)) *bytes += st.st_size;
        return 0;
    }
    DIR *dir = opendir(path);
    if (!dir) return -1;
    struct dirent *entry;
    int result = 0;
    while ((entry = readdir(dir))) {
        if (!strcmp(entry->d_name, ".") || !strcmp(entry->d_name, "..")) continue;
        char child[4096];
        int n = snprintf(child, sizeof child, "%s/%s", path, entry->d_name);
        if (n < 0 || (size_t)n >= sizeof child || remove_tree(child, bytes) != 0) {
            result = -1; break;
        }
    }
    closedir(dir);
    return result == 0 ? rmdir(path) : result;
}

static void do_rm_xbox_partial(const char *path)
{
    const char *prefix = "/data/playden-games/xbox-";
    size_t n = strlen(prefix);
    if (strlen(path) != n + 12 || strncmp(path, prefix, n)) {
        say("  refused: expected canonical Xbox content root"); return;
    }
    for (size_t i = n; i < n + 12; ++i) {
        if (!((path[i] >= 'a' && path[i] <= 'z') || (path[i] >= '0' && path[i] <= '9'))) {
            say("  refused: invalid product id"); return;
        }
    }
    struct stat st;
    if (lstat("/data/playden-games", &st) != 0 || !S_ISDIR(st.st_mode)) {
        say("  refused: invalid content parent"); return;
    }
    long long bytes = 0;
    if (remove_tree(path, &bytes) != 0) say("  removal failed: %s", strerror(errno));
    else say("  removed-tree bytes=%lld", bytes);
}

static int copy_file(const char *src, const char *dst, mode_t mode)
{
    return copy_file_mode(src, dst, mode, 0);
}

static void do_get(const char *src, const char *name, int raw)
{
    char dst[1024];
    if (strchr(name, '/')) {
        say("  refused: name has a slash");
        return;
    }
    snprintf(dst, sizeof dst, DROP "/%s", name);
    copy_file_mode(src, dst, 0644, raw);
}

int main(void)
{
    char line[4096];
    FILE *cmds;

    out = fopen(DROP "/uidcp.log", "w");
    if (!out)
        return 1;
    say("uidcp: uid %d euid %d", getuid(), geteuid());
    cmds = fopen(DROP "/uidcp.txt", "r");
    if (!cmds) {
        say("no " DROP "/uidcp.txt: %s", strerror(errno));
        fclose(out);
        return 1;
    }
    if (seteuid(1) != 0) {
        say("seteuid(1): %s", strerror(errno));
        fclose(cmds);
        fclose(out);
        return 1;
    }
    say("now euid %d", geteuid());
    while (fgets(line, sizeof line, cmds)) {
        char *cmd, *a, *b, *save = NULL;
        line[strcspn(line, "\r\n")] = 0;
        if (!line[0] || line[0] == '#')
            continue;
        cmd = strtok_r(line, "\t", &save);
        a = strtok_r(NULL, "\t", &save);
        b = strtok_r(NULL, "\t", &save);
        say("> %s %s%s%s", cmd, a ? a : "", b ? " -> " : "", b ? b : "");
        if (!strcmp(cmd, "ls") && a)
            do_ls(a);
        else if (!strcmp(cmd, "cp") && a && b)
            do_cp(a, b, 0644);
        else if (!strcmp(cmd, "cp-executable") && a && b)
            do_cp(a, b, 0755);
        else if (!strcmp(cmd, "cp-private") && a && b)
            do_cp(a, b, 0600);
        else if (!strcmp(cmd, "get") && a && b)
            do_get(a, b, 0);
        else if (!strcmp(cmd, "get-raw") && a && b)
            do_get(a, b, 1);
        else if (!strcmp(cmd, "mkdir") && a)
            do_mkdir(a);
        else if (!strcmp(cmd, "rm-xbox-partial") && a)
            do_rm_xbox_partial(a);
        else if (!strcmp(cmd, "rm") && a)
            do_rm(a);
        else
            say("  unknown command");
    }
    seteuid(0);
    say("done");
    fclose(cmds);
    fclose(out);
    return 0;
}
