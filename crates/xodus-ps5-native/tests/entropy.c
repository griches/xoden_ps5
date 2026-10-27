/* Adapter-contract test only. Deterministic mock is never part of PS5 builds. */
#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <sys/types.h>
int getentropy(void *, size_t);
ssize_t getrandom(void *, size_t, unsigned int);
static int fail, calls;
int sceRandomGetRandomNumber(uint8_t *out, size_t size) {
    assert(size && size <= 32); ++calls;
    if (fail) return -1;
    memset(out,0xa5,size); return 0;
}
int main(void) {
    uint8_t bytes[257] = {0};
    assert(getentropy(bytes,256) == 0 && calls == 8);
    for (int i = 0; i < 256; ++i) assert(bytes[i] == 0xa5);
    assert(bytes[256] == 0);
    assert(getentropy(bytes,257) == -1 && errno == EIO && calls == 8);
    assert(getentropy(NULL,0) == 0 && calls == 8);
    assert(getentropy(NULL,1) == -1 && errno == EFAULT && calls == 8);
    assert(getrandom(bytes,257,0) == 257 && calls == 17 && bytes[256] == 0xa5);
    assert(getrandom(bytes,1,1) == -1 && errno == EINVAL && calls == 17);
    fail = 1;
    assert(getentropy(bytes,1) == -1 && errno == EIO && calls == 18);
    assert(getrandom(bytes,1,0) == -1 && errno == EIO && calls == 19);
    return 0;
}
