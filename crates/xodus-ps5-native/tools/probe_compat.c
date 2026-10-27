/* Experimental native job link support, not a complete PS5 runtime. */
#include <pthread.h>
#include <pthread_np.h>
#include <stddef.h>
#include <sys/stat.h>
struct dl_phdr_info;
/* Disable Rust backtrace module enumeration in this isolated test. */
int dl_iterate_phdr(int (*callback)(struct dl_phdr_info *, size_t, void *), void *data) {
    (void)callback; (void)data; return 0;
}
int pthread_setname_np(pthread_t thread, const char *name) {
    pthread_set_name_np(thread,name); return 0;
}
_Static_assert(sizeof(struct stat) == 120, "requires PS5/FreeBSD11 stat layout");
int xodus_probe_stat(const char *path, struct stat *out) { return stat(path,out); }
int xodus_probe_fstat(int fd, struct stat *out) { return fstat(fd,out); }
__asm__(".symver xodus_probe_stat,stat@FBSD_1.0");
__asm__(".symver xodus_probe_fstat,fstat@FBSD_1.0");
#include <dirent.h>
#include <sys/event.h>
_Static_assert(sizeof(struct dirent) == 264, "FreeBSD11 directory entry");
_Static_assert(offsetof(struct dirent,d_fileno) == 0, "directory inode offset");
_Static_assert(offsetof(struct dirent,d_reclen) == 4, "directory record size offset");
_Static_assert(offsetof(struct dirent,d_type) == 6, "directory type offset");
_Static_assert(offsetof(struct dirent,d_namlen) == 7, "directory name length offset");
_Static_assert(offsetof(struct dirent,d_name) == 8, "directory name offset");
_Static_assert(sizeof(struct kevent) == 32, "FreeBSD11 event layout");
_Static_assert(offsetof(struct kevent,ident) == 0, "event identity offset");
_Static_assert(offsetof(struct kevent,filter) == 8, "event filter offset");
_Static_assert(offsetof(struct kevent,flags) == 10, "event flags offset");
_Static_assert(offsetof(struct kevent,fflags) == 12, "event filter flags offset");
_Static_assert(offsetof(struct kevent,data) == 16, "event data offset");
_Static_assert(offsetof(struct kevent,udata) == 24, "event user data offset");
int xodus_probe_lstat(const char *path, struct stat *out) { return lstat(path,out); }
struct dirent *xodus_probe_readdir(DIR *dir) { return readdir(dir); }
int xodus_probe_kevent(int queue, const struct kevent *changes, int nchanges,
                      struct kevent *events, int nevents, const struct timespec *timeout) {
    return kevent(queue,changes,nchanges,events,nevents,timeout);
}
__asm__(".symver xodus_probe_lstat,lstat@FBSD_1.0");
__asm__(".symver xodus_probe_readdir,readdir@FBSD_1.0");
__asm__(".symver xodus_probe_kevent,kevent@FBSD_1.0");

#ifndef XODUS_NATIVE_TITLE
/* The link-only payload probe has no title volume policy. Fail honestly;
 * Playden supplies its validated volume query in the actual native title. */
#include <stdint.h>
int xodus_ps5_available_space(const char *path, uint64_t *bytes) {
    (void)path; (void)bytes; return -1;
}
/* The link-only payload never writes account credentials. The actual title
 * supplies its existing policy-checked atomic private-file writer. */
int xodus_ps5_commit_keyring(const uint8_t *bytes, size_t size) {
    (void)bytes; (void)size; return -1;
}
#endif
