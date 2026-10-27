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
/* Optional job-engine feature. Host supplies approved absolute storage roots
 * and its packaged CA bundle. Start copies paths and schedules IO on a worker.
 * No request-controlled paths. Existing private xodus-keyring.ron is required.
 * Keep the library loaded until a terminal poll and successful release.
 * Return codes: 0 success; -1 arguments/ABI; -2 stale handle; -3 busy;
 * -4 worker unavailable. On error output arguments remain unchanged. */
typedef struct XodusJobConfig {
    const uint8_t *root; size_t root_len;
    const uint8_t *games; size_t games_len;
    const uint8_t *trust; size_t trust_len;
} XodusJobConfig;
typedef struct XodusJobSnapshot {
    uint32_t abi_version, struct_size, state, flags;
    uint64_t completed_bytes, total_bytes, bytes_per_second, sequence;
    char phase[32], error[32];
} XodusJobSnapshot;
/* States 1 starting, 2 running, 3 stopping; terminal: 4 complete, 5 failed,
 * 6 cancelled. Only terminal permits cleanup. Complete means job protocol
 * finished: read job-result.json to distinguish prepared/package-choice/etc.
 * Flags: bit 0 total known, bit 1 paused. Strings contain no credentials/URLs.
 * Cancel preserves partial files; release refuses a nonterminal job. */
int32_t xodus_job_start(const XodusJobConfig *, size_t, uint32_t, uint64_t *handle);
int32_t xodus_job_poll(uint64_t handle, XodusJobSnapshot *, size_t, uint32_t);
int32_t xodus_job_cancel(uint64_t handle);
int32_t xodus_job_release(uint64_t handle);
/* Host import used by credential writes: atomically commit the private keyring
 * snapshot at the host's approved path. Never log or publish these bytes.
 * 0 confirms atomic private-file publication; any other result preserves the
 * old file. The link-only probe refuses writes. */
int xodus_ps5_commit_keyring(const uint8_t *bytes, size_t length);
#ifdef __cplusplus
}
#endif
#endif
