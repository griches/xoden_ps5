/* Kernel-device contract: build with -DXODUS_TEST_RANDOM_DEVICE. */
#include <assert.h>
#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>
int getentropy(void *, size_t);
ssize_t getrandom(void *, size_t, unsigned int);
void __wrap_arc4random_buf(void *, size_t);
int main(void) {
    uint8_t bytes[258] = {0}; bytes[257] = 0x5a;
    assert(getentropy(bytes,256) == 0 && bytes[257] == 0x5a);
    assert(getentropy(bytes,257) == -1 && errno == EIO);
    assert(getentropy(NULL,0) == 0);
    assert(getentropy(NULL,1) == -1 && errno == EFAULT);
    __wrap_arc4random_buf(bytes,257);
    assert(bytes[257] == 0x5a);
    assert(getrandom(bytes,257,0) == 257 && bytes[257] == 0x5a);
    assert(getrandom(bytes,1,1) == -1 && errno == EINVAL);
    return 0;
}
