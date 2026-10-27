/* PS5 TLS/Rust entropy adapter. Uses the SDK's libSceRandom entry point;
 * never libc arc4random (which can abort) or a non-cryptographic fallback. */
#include <errno.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/types.h>
#if defined(__FreeBSD__) || defined(XODUS_TEST_RANDOM_MODULE)
#include <pthread.h>
extern int sceKernelLoadStartModule(const char *, size_t, const void *, unsigned int, const void *, int *);
extern int sceKernelDlsym(int, const char *, void **);
static pthread_once_t random_once = PTHREAD_ONCE_INIT;
static int random_status;
int xodus_ps5_entropy_status(void) { return random_status; }
static int (*random_fill)(uint8_t *, size_t);
static void load_random(void) {
    int result = 0;
    const int handle = sceKernelLoadStartModule("/system/common/lib/libSceRandom.sprx",0,NULL,0,NULL,&result);
    random_status = handle;
    void *symbol = NULL;
    if (handle >= 0 && (random_status = sceKernelDlsym(handle,"sceRandomGetRandomNumber",&symbol)) == 0 && symbol)
        random_fill = (int (*)(uint8_t *, size_t))symbol;
}
static int random_bytes(uint8_t *buffer, size_t size) {
    if (pthread_once(&random_once,load_random) != 0 || !random_fill) return -1;
    return random_fill(buffer,size);
}
#else
extern int sceRandomGetRandomNumber(uint8_t *buffer, size_t size);
#define random_bytes sceRandomGetRandomNumber
int xodus_ps5_entropy_status(void) { return 0; }
#endif
static int fill(void *buffer, size_t length) {
    if (!buffer && length) { errno = EFAULT; return -1; }
    uint8_t *cursor = buffer;
    while (length) {
        const size_t count = length < 32 ? length : 32;
        if (random_bytes(cursor,count) != 0) { errno = EIO; return -1; }
        cursor += count; length -= count;
    }
    return 0;
}
int getentropy(void *buffer, size_t length) {
    if (length > 256) { errno = EIO; return -1; }
    return fill(buffer,length);
}
ssize_t getrandom(void *buffer, size_t length, unsigned int flags) {
    /* The Rust dependencies request flags=0. Refuse unsupported semantics. */
    if (flags || length > SSIZE_MAX) { errno = EINVAL; return -1; }
    return fill(buffer,length) == 0 ? (ssize_t)length : -1;
}

/* Rust's FreeBSD std uses this for HashMap/Tokio seeds, independently of
 * getrandom. Its void API cannot report entropy failure: fail closed. */
void arc4random_buf(void *buffer, size_t length) {
    if (fill(buffer,length) != 0) abort();
}
