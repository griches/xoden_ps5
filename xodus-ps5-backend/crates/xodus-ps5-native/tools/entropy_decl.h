#ifndef __ASSEMBLER__
#include <stddef.h>
/* Build declaration only. Networking needs a verified PS5 CSPRNG adapter
 * before any function using this symbol may be linked or executed. */
int getentropy(void *buffer, size_t length);
#endif
