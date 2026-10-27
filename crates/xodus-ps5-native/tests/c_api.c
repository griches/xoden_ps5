#include "xodus_ps5_native.h"
#include <assert.h>
#include <string.h>
#include <stdio.h>
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
    return 0;
}
