/* PS5 TLS/Rust entropy adapter. Uses the SDK's libSceRandom entry point;
 * never libc arc4random (which can abort) or a non-cryptographic fallback. */
#include <errno.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>
extern int sceRandomGetRandomNumber(uint8_t *buffer, size_t size);
static int fill(void *buffer, size_t length) {
    if (!buffer && length) { errno = EFAULT; return -1; }
    uint8_t *cursor = buffer;
    while (length) {
        const size_t count = length < 32 ? length : 32;
        if (sceRandomGetRandomNumber(cursor,count) != 0) { errno = EIO; return -1; }
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
