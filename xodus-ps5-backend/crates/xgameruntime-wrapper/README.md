# PS5 in-game runtime bridge

This ports Gary's existing in-process Windows bridge into the active backend
workspace so it uses the same credential types as the native app. Build with
`windows/build.sh` inside the configured Lima PS5 build VM. It forwards runtime
exports to `xgameruntime_impl.dll`.

Normal operation retains the existing STS sign-in path. The exact opt-in
environment variable `XODUS_TEST_PHONE_LOGIN=1` selects a separate phone-login
candidate stored by the native app. Missing or invalid candidates fail; there is
no fallback to the active account. This test mode never saves its keyring
snapshot or adopts the candidate. Refreshed candidate credentials remain in
process memory.

The test requests app-level Xbox service tickets. A direct OAuth refresh into
MIO's own MSA application was rejected by Microsoft. Whether XAL inside a game
accepts the service ticket remains unverified. OAuth CID and Xbox XUID values are
not presented as Store PUIDs. Do not make this the default runtime path until
actual game sign-in is accepted.

Host regression tests cover no fallback, invalid application identity before
network access, and omission of unverified account identity fields. The Windows
DLL cross-build passed. The test DLL has not been deployed.
