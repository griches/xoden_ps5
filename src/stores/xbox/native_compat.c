// SPDX-License-Identifier: GPL-3.0-or-later
/* Adapters for the native-title link (the payload SDK supplies these itself). */
#include <dirent.h>
#include <errno.h>
#include <stddef.h>
#include <string.h>

int dirfd(DIR *directory) {
    /* Same ABI as PS5_PayloadSDK/libc/dirfd.c: the descriptor is the first
     * field of the system DIR. Copy avoids imposing a different struct type. */
    if (!directory) { errno = EINVAL; return -1; }
    int fd;
    memcpy(&fd,directory,sizeof(fd));
    return fd;
}

int minherit(void *address, size_t length, int inheritance) {
    /* This native title has no fork-memory inheritance adapter. Report that
     * honestly: AWS-LC's fork_ube_detect.c treats failure as unavailable and
     * uses its normal entropy path. Never pretend inherited memory was reset. */
    (void)address; (void)length; (void)inheritance;
    errno = ENOSYS;
    return -1;
}
