# Remaining work

Updated 2026-10-27. The [work record](xodus-games/GAMEPASS-PS5-RECORD.md) holds
test evidence; this page tracks outstanding functionality.

## Priorities

- [ ] Fresh Microsoft sign-in on PS5, using a phone/device-code flow if its
  credentials can support the PC game licensing requirements.
- [ ] Complete PC-license and game-runtime validation of phone login. OAuth,
  Xbox profile/marketplace tickets and Store-scope refresh passed in isolation.
  Keep the working account until candidate validation passes; then test
  sign-out, fresh sign-in, refresh, cancellation, expiry and account replacement.
- [ ] Continue tuning direct-download throughput. Deployed bounded parallel
  ranges measured 12.3–15.5 MB/s during MIO, including a single-large-file window,
  compared with earlier 6.3 MB/s on Elder Gods. These are different games, not
  a controlled A/B comparison. CPU/network worker tuning remains to be tested.
- [ ] Make completed-install registration recover reliably without a reboot;
  show a useful reason when the loader is stalled.
- [ ] Finish multi-job queue scheduling and acceptance for interruption,
  restart, insufficient storage and cleanup.
- [ ] Complete standalone app identity, separate title/package assets, settings
  and update ownership. Remove remaining Steam-specific product flows.
- [ ] Verify generated PS5 Home `icon0` / `pic0` artwork on new installs and
  refresh already registered titles. Generation and owned disk-cache updates
  are deployed; existing ShellUI artwork remained cached in the visual check.
- [ ] Verify subscription expiry and account changes end to end, including
  launch-time behavior; do not claim enforcement beyond the checks actually tested.
- [ ] Expand compatibility tests, including save/reload and longer sessions.
- [ ] Connect app-side compatibility reporting to this repository only if/when
  implemented. Currently ratings are local and GitHub reports are manual.

## Already implemented

- Full catalogue browsing/search, Game Pass-only Home/Library and green styling.
- Persistent cover caching and proportional Game Pass artwork on game details.
- Bounded parallel HTTP ranges with cancellation, ordered delivery and resume tests.
- Generated icon0/pic0 artwork and bounded owned-title disk-cache updates.
- Game Pass startup/settings no longer restore or display the Steam account.
- Controller reconnection after virtual-pad handover.
- Package choices and size review before downloads.
- Native PS5 downloads; pause/restart/resume progress persistence and cancel cleanup.
- Automatic Xbox runtime preparation, shipping-executable selection and Home title setup.
- Minishoot and Digging gameplay acceptance; Elder Gods launch reported after reboot.

Gary's status questions do not pause work. Give a concise progress update at least
once per minute during active work, and always release the virtual controller
after brief console testing.
