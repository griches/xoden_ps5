#ifndef XODUS_PS5_NATIVE_H
#define XODUS_PS5_NATIVE_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define XODUS_NATIVE_ABI 1u
#define XODUS_XVD_HEADER_SIZE 4096u
/* Header inspection only: success does not establish entitlement, signature
 * validity, complete content, or compatibility. No IO or credentials involved. */
typedef struct XodusPackageHeader {
    uint32_t abi_version;
    uint32_t struct_size;
    uint64_t drive_bytes;
    uint64_t user_data_bytes;
    uint8_t content_id[16];
    uint32_t format_version;
    uint32_t volume_flags;
} XodusPackageHeader;
/* Caller supplies valid non-overlapping buffers. Output is unchanged on error.
 * 0 = parsed, -1 = bad arguments/ABI, -2 = invalid or unsupported header. */
int32_t xodus_inspect_package_header(const uint8_t *bytes, size_t length,
                                    XodusPackageHeader *output, size_t output_size,
                                    uint32_t abi_version);
#ifdef __cplusplus
}
#endif
#endif
