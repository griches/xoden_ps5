#include <assert.h>
#include <stdint.h>
#include <string.h>
int xodus_ps5_resolve_ipv4(const char *, unsigned char *);
static int fail, pools, resolvers;
int sceNetPoolCreate(const char *n, int size, int flags) {
    (void)n; assert(size == 0x4000 && flags == 0);
    if (fail == 1) return -1;
    ++pools; return 10;
}
int sceNetPoolDestroy(int id) { assert(id == 10); --pools; return 0; }
int sceNetResolverCreate(const char *n, int pool, int flags) {
    (void)n; assert(pool == 10 && flags == 0);
    if (fail == 2) return -1;
    ++resolvers; return 11;
}
int sceNetResolverDestroy(int id) { assert(id == 11); --resolvers; return 0; }
int sceNetResolverStartNtoa(int id, const char *host, uint32_t *out, int timeout, int retries, int flags) {
    assert(id == 11 && !strcmp(host,"example.com"));
    assert(timeout == 5000000 && retries == 0 && flags == 0);
    const unsigned char address[] = {192,0,2,1};
    memcpy(out,address,4);
    return fail == 3 ? -1 : 0;
}
int main(void) {
    unsigned char out[4] = {0};
    assert(xodus_ps5_resolve_ipv4(0,out) != 0);
    assert(xodus_ps5_resolve_ipv4("",out) != 0);
    assert(xodus_ps5_resolve_ipv4("example.com",0) != 0);
    for (fail=1; fail<=3; ++fail) {
        assert(xodus_ps5_resolve_ipv4("example.com",out) != 0);
        assert(!pools && !resolvers && out[0] == 0);
    }
    fail=0;
    assert(xodus_ps5_resolve_ipv4("example.com",out) == 0);
    assert(!pools && !resolvers && out[0] == 192 && out[1] == 0 && out[2] == 2 && out[3] == 1);
    return 0;
}
