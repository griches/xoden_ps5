# Remaining work

Updated 2026-10-27. The [work record](xodus-games/GAMEPASS-PS5-RECORD.md) holds
test evidence; this page tracks outstanding functionality.

## Priorities

- [ ] Complete the single **Sign in with Microsoft** flow for account, library,
  licensing and game authorisation. The PS5 performs the original browser
  broker exchange through a paired local computer page and saves one session;
  library loading follows automatically. The separate phone/computer choice
  has been removed. Fresh approval, Xbox identity verification, session save
  and automatic library refresh passed on the PS5 at 23:33Z. A private inactive
  approval cache avoids repeating browser login after failed verification.
  Gary also confirmed Call of the Elder Gods is signed in and its existing saves
  are visible. Verify MIO and expiry/account-change behavior before considering
  this complete across games. Bounded automatic verification retries are
  installed. Fresh browser approval, session save and automatic library refresh
  completed in one menu attempt on 2026-10-28.
- [ ] Verify sign-in persistence, expiry, account changes and game runtime end
  to end. Earlier device-code login proved library, subscription and PC licence
  access but not MIO's game grant. It is not the user-facing sign-in route.
  Preserve installed games/saves and never restore the previously cleared
  session as a substitute for fresh approval.
- [ ] Continue tuning direct-download throughput. Deployed bounded parallel
  ranges measured 12.3–15.5 MB/s during MIO, including a single-large-file window,
  compared with earlier 6.3 MB/s on Elder Gods. These are different games, not
  a controlled A/B comparison. CPU/network worker tuning remains to be tested.
- [ ] Make completed-install registration recover reliably without a reboot;
  show a useful reason when the loader is stalled.
- [ ] Finish multi-job queue scheduling and acceptance for interruption,
  restart, insufficient storage and cleanup.
- [ ] Complete standalone app identity, separate title/package assets, settings
  and update ownership. XoDen's visible name, original icon/background and larger
  pairing screen are installed; existing IDs/storage retain compatibility.
  Remove remaining Steam-specific product flows.
- [ ] Refresh generated PS5 Home artwork on already registered titles. New
  MIO installation shows icon0 and pic0 correctly on the actual Home screen;
  Elder Gods artwork remains cached by ShellUI despite owned disk-cache updates.
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
- Unified Microsoft browser approval, verified session persistence and automatic
  Xbox library loading accepted on the actual PS5.
- Call of the Elder Gods game sign-in and existing-save visibility confirmed by
  Gary with the freshly approved browser session.
- Native account reset retains installed games, saves and registered device identity.
- Phone sign-in QR display and safe cancellation accepted on the actual PS5.
- Normal game exit returns to PS5 Home; MIO's in-game 3840x2160 option verified.
- Package choices and size review before downloads.
- Native PS5 downloads; pause/restart/resume progress persistence and cancel cleanup.
- Automatic Xbox runtime preparation, shipping-executable selection and Home title setup.
- Minishoot and Digging gameplay acceptance; Elder Gods launch reported after reboot.

Gary's status questions do not pause work. Give a concise progress update at least
once per minute during active work, and always release the virtual controller
after brief console testing.
