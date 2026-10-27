/* Experimental header-only link support, not a complete PS5 runtime. */
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
