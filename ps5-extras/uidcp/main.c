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
 *   get <src> <name>   copies src to /data/xodus-drop/<name> (mode 0644)
 *   mkdir <dir>        dir must be under /data/playden/ (mode 0700)
 *   rm <path>          path must be under /data/playden/
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

static void say(const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    vfprintf(out, fmt, ap);
    va_end(ap);
    fputc('\n', out);
    fflush(out);
}

static int copy_file(const char *src, const char *dst, mode_t mode)
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

static void do_get(const char *src, const char *name)
{
    char dst[1024];
    if (strchr(name, '/')) {
        say("  refused: name has a slash");
        return;
    }
    snprintf(dst, sizeof dst, DROP "/%s", name);
    copy_file(src, dst, 0644);
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
        else if (!strcmp(cmd, "cp-private") && a && b)
            do_cp(a, b, 0600);
        else if (!strcmp(cmd, "get") && a && b)
            do_get(a, b);
        else if (!strcmp(cmd, "mkdir") && a)
            do_mkdir(a);
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
