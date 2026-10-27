# Remaining work

Updated 2026-10-27. The [work record](xodus-games/GAMEPASS-PS5-RECORD.md) holds
test evidence; this page tracks outstanding functionality.

## Priorities

- [ ] Fresh Microsoft sign-in on PS5, using a phone/device-code flow if its
  credentials can support the PC game licensing requirements.
- [ ] Validate the new sign-in without replacing the working account; then test
  sign-out, fresh sign-in, refresh, cancellation, expiry and account replacement.
- [ ] Diagnose and improve direct-download throughput. Measured content growth
  was about 6.3 MB/s; read-only local FTP achieved 24.4 MB/s. The bottleneck is
  not yet established, and no speed improvement has been deployed.
- [ ] Evaluate bounded parallel ranges for a single large package/file, with
  integrity, cancellation and resume preserved. Current file concurrency does
  not parallelize the one large active file.
- [ ] Make completed-install registration recover reliably without a reboot;
  show a useful reason when the loader is stalled.
- [ ] Finish multi-job queue scheduling and acceptance for interruption,
  restart, insufficient storage and cleanup.
- [ ] Complete standalone app identity, separate title/package assets, settings
  and update ownership. Remove remaining Steam-specific product flows.
- [ ] Generate PS5 Home `icon0` / `pic0` assets from each game's downloaded artwork.
- [ ] Verify subscription expiry and account changes end to end, including
  launch-time behavior; do not claim enforcement beyond the checks actually tested.
- [ ] Expand compatibility tests, including save/reload and longer sessions.
- [ ] Connect app-side compatibility reporting to this repository only if/when
  implemented. Currently ratings are local and GitHub reports are manual.

## Already implemented

- Full catalogue browsing/search, Game Pass-only Home/Library and green styling.
- Persistent cover caching.
- Package choices and size review before downloads.
- Native PS5 downloads; pause/restart/resume progress persistence and cancel cleanup.
- Automatic Xbox runtime preparation, shipping-executable selection and Home title setup.
- Minishoot and Digging gameplay acceptance; Elder Gods launch reported after reboot.

Gary's status questions do not pause work. Give a concise progress update at least
once per minute during active work, and always release the virtual controller
after brief console testing.
