/* PS5 TLS/Rust entropy adapter. Uses the kernel random device already used by
 * Playden. Never libc arc4random or a non-cryptographic fallback. */
#include <errno.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/types.h>
#if defined(__FreeBSD__) || defined(XODUS_TEST_RANDOM_DEVICE)
#include <fcntl.h>
#include <unistd.h>
int xodus_ps5_entropy_status(void) { return errno; }
static int random_bytes(uint8_t *buffer, size_t size) {
    const int fd = open("/dev/urandom",O_RDONLY | O_CLOEXEC);
    if (fd < 0) return -1;
    size_t done = 0;
    while (done < size) {
        const ssize_t count = read(fd,buffer + done,size - done);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) { const int error = count == 0 ? EIO : errno; close(fd); errno = error; return -1; }
        done += (size_t)count;
    }
    close(fd);
    return 0;
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
