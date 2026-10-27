#include "xodus_ps5_native.h"
#include <assert.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#ifdef XODUS_JOB_ENGINE
_Static_assert(sizeof(XodusJobConfig) == 48, "job config ABI");
_Static_assert(sizeof(XodusJobSnapshot) == 112, "job snapshot ABI");
_Static_assert(offsetof(XodusJobSnapshot, phase) == 48, "job phase offset");
static void job_test(void) {
    uint64_t handle = 99;
    XodusJobSnapshot state = {0};
    assert(xodus_job_start(NULL,0,XODUS_NATIVE_ABI,&handle) == -1 && handle == 99);
    assert(xodus_job_poll(0,&state,sizeof(state),XODUS_NATIVE_ABI) == -2);
    char root[] = "/tmp/xodus-job-test-XXXXXX";
    assert(mkdtemp(root));
    char request[256], lock[256];
    snprintf(request,sizeof(request),"%s/job-request.json",root);
    snprintf(lock,sizeof(lock),"%s/job.lock",root);
    FILE *file = fopen(request,"wb"); assert(file);
    assert(fputs("{\"schema\":1,\"requestId\":\"11111111-1111-4111-8111-111111111111\",\"operation\":\"refresh\",\"market\":\"GB\"}",file) >= 0);
    assert(fclose(file) == 0);
    const char *games = "/tmp/xodus-unused-games", *trust = "/nonexistent/xodus-test-trust.pem";
    XodusJobConfig config = {(const uint8_t *)root,strlen(root),(const uint8_t *)games,strlen(games),(const uint8_t *)trust,strlen(trust)};
    assert(xodus_job_start(&config,sizeof(config),XODUS_NATIVE_ABI,&handle) == 0);
    uint64_t unchanged = 99;
    assert(xodus_job_start(&config,sizeof(config),XODUS_NATIVE_ABI,&unchanged) == -3 && unchanged == 99);
    for (int i = 0; i < 1000; ++i) {
        assert(xodus_job_poll(handle,&state,sizeof(state),XODUS_NATIVE_ABI) == 0);
        if (state.state >= 4) break;
        usleep(10000);
    }
    assert(state.state == 5 && strcmp(state.error,"trust-open") == 0);
    assert(xodus_job_release(handle) == 0);
    assert(xodus_job_cancel(handle) == -2);
    assert(xodus_job_poll(handle,&state,sizeof(state),XODUS_NATIVE_ABI) == -2);
    assert(unlink(request) == 0); assert(unlink(lock) == 0); assert(rmdir(root) == 0);
    puts("native Xbox C job lifecycle test passed");
}
#endif
_Static_assert(sizeof(XodusPackageHeader) == 48, "native header ABI size");
_Static_assert(offsetof(XodusPackageHeader, content_id) == 24, "native header ABI offset");
int main(void) {
    uint8_t bytes[XODUS_XVD_HEADER_SIZE] = {0};
    memcpy(bytes + 0x200, "msft-xvd", 8);
    const uint8_t epoch[8] = {0x00,0x80,0x3e,0xd5,0xde,0xb1,0x9d,0x01};
    memcpy(bytes + 0x210, epoch, sizeof(epoch));
    bytes[0x219] = 0x10; /* 4096 drive bytes */
    XodusPackageHeader output = {0};
    assert(xodus_inspect_package_header(bytes,sizeof(bytes),&output,sizeof(output),XODUS_NATIVE_ABI) == 0);
    assert(output.abi_version == XODUS_NATIVE_ABI && output.struct_size == sizeof(output));
    assert(output.drive_bytes == 4096);
    const XodusPackageHeader previous = output;
    bytes[0x200] = 0;
    assert(xodus_inspect_package_header(bytes,sizeof(bytes),&output,sizeof(output),XODUS_NATIVE_ABI) == -2);
    assert(memcmp(&previous,&output,sizeof(output)) == 0);
    puts("native Xbox C ABI header test passed");
    #ifdef XODUS_JOB_ENGINE
    job_test();
    #endif
    return 0;
}
