/* PS5 DNS bridge: output is four network-order octets, never a host integer. */
#include <stdint.h>
#include <string.h>
extern int sceNetPoolCreate(const char *, int, int);
extern int sceNetPoolDestroy(int);
extern int sceNetResolverCreate(const char *, int, int);
extern int sceNetResolverDestroy(int);
extern int sceNetResolverStartNtoa(int, const char *, uint32_t *, int, int, int);

int xodus_ps5_resolve_ipv4(const char *host, unsigned char *out) {
    if (!host || !out || !*host || strlen(host) > 253) return -1;
    /* Playden owns sceNetInit/Term for the lifetime of the title. */
    int pool = sceNetPoolCreate("xodus-dns", 0x4000, 0);
    if (pool < 0) return -1;
    int resolver = sceNetResolverCreate("xodus-dns", pool, 0);
    int result = -1;
    uint32_t address = 0;
    if (resolver >= 0) {
        result = sceNetResolverStartNtoa(resolver, host, &address, 5000000, 0, 0);
        if (result == 0) memcpy(out, &address, 4);
        sceNetResolverDestroy(resolver);
    }
    sceNetPoolDestroy(pool);
    return result;
}
