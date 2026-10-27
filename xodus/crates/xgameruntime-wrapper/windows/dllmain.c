/* Starts the in-process xodus service; the exports are forwarders (xgameruntime.def). */
#include <windows.h>

void xodus_wrapper_start(void);

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved)
{
    (void)reserved;
    if (reason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(instance);
        xodus_wrapper_start();
    }
    return TRUE;
}
