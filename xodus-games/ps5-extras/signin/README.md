# Microsoft browser sign-in for Game Pass on PS5

Development helper for the console's single Microsoft sign-in flow, covering
the account, library, licensing and game authorisation. Node.js 24+ and Google
Chrome are required. It does not
read the computer's Xodus Keychain or copy device credentials between machines.
The local page can open in Safari or any default browser. Microsoft login uses
a separate temporary Chrome window; changing the default browser is unnecessary.

1. On the PS5, open **Settings → Xbox account → Sign in with Microsoft**.
2. On the Mac, double-click `start.command`, or run `npm ci --ignore-scripts`,
   `npm run build` and `npm start` here. Open <http://127.0.0.1:47832>.
3. Enter the PS5's local IPv4 address and the 16-character pairing code on the TV.
4. Complete Microsoft's sign-in in the temporary Chrome window using the same
   Microsoft account as the currently connected console account. To switch
   accounts, sign out on the console first.
5. Wait for the console's saved confirmation before closing the helper.

The console performs the original Xodus hosted-login exchange using its own
registered device. It verifies the fresh browser account and atomically saves
one session used by library access, licensing and the game bridge. Any previous
phone-only session is superseded. The app refreshes the library automatically
and revalidates installed-game bindings against fresh licences. Game files,
saves and device keys are retained. Sign-out clears the account. In-game
acceptance must be checked separately;
the helper's successful save is not proof that a game has signed in.

The local page listens only on `127.0.0.1:47832` with Host, Origin and per-process
API-token checks. The console opens TLS port 47834 for this operation only, for
at most ten minutes. The pairing code checks a 64-bit SHA-256 certificate
fingerprint prefix; no application data is sent during certificate discovery.
The following connection verifies that certificate before carrying Microsoft
callbacks. Chrome uses a disposable profile and private inherited pipes; its
normal profile is not accessed. No account tokens are saved on the computer.

The browser grant is projected into a memory-only game bridge; refreshed runtime
credentials are not yet persisted between launches. Library refresh can renew
the main saved session. Long-term renewal and a completely empty console's
first device setup remain unverified.

Offline tests, using synthetic callbacks and a temporary local TLS fixture:

```sh
npm ci --ignore-scripts
npm run build
npm test
```

The browser adapter is derived from Xodus Nexus under MIT; see
`browser/NOTICE.md` and `browser/LICENSE`. Build output and node_modules are
ignored. No console account files belong in this directory.
