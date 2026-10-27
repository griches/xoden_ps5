# PS5 in-game runtime bridge

This ports Gary's existing in-process Windows bridge into the active backend
workspace so it uses the same credential types as the native app. Build with
`windows/build.sh` inside the configured Lima PS5 build VM. It forwards runtime
exports to `xgameruntime_impl.dll`.

An active native phone session selects the OAuth bridge. Without it, normal
operation retains the existing STS path. The exact opt-in environment variable
`XODUS_TEST_PHONE_LOGIN=1` also permits a separate verified phone candidate for
development tests. Invalid active sessions fail without legacy fallback. Both
phone paths use a memory-only store, including on Drop; game processes never
rewrite the shared account snapshot. Refreshed credentials stay in memory.

Phone HTTP requests use a 15-second deadline, refuse redirects, and verify TLS
against the native app's packaged trust/cacert.pem. XODUS_CA_BUNDLE can select
another local CA bundle. No certificate verification bypass is used.

The test requests app-level Xbox service tickets. A direct OAuth refresh into
MIO's own MSA application was rejected by Microsoft. Whether XAL inside a game
accepts the service ticket remains unverified. OAuth CID and Xbox XUID values are
not presented as Store PUIDs. Library/profile/PC licensing with this session has passed on the actual PS5.
In-game acceptance remains under investigation; the first MIO launch stalled
and reported bridge request failures.

Host regression tests cover no fallback, invalid application identity before
network access, and omission of unverified account identity fields. The Windows
DLL cross-build passed. The app can bundle the bridge with PLAYDEN_XBOX_WRAPPER_DLL so its managed
installer copies the validated PE into a stopped bottle. The initial bridge
was deployed; supported-scope and packaged-trust corrections are being tested.
