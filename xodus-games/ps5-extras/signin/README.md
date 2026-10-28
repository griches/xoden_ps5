# Microsoft browser sign-in for Game Pass on PS5

Separate computer helper for XoDen's single Microsoft sign-in flow, covering
the account, library, licensing and game authorisation. Node.js 24+ and Google
Chrome are required. It does not
read the computer's Xodus Keychain or copy device credentials between machines.
The local page can open in Safari or any default browser. Microsoft login uses
a separate temporary Chrome window; changing the default browser is unnecessary.

## Run the release helper

The release download is **xoden-signin-helper-0.1.0.zip**. It includes the built
browser adapter. Install **Node.js 24+ and Google Chrome** separately; this is
not a standalone Windows executable. **macOS is tested; Windows is unverified.**
You need the helper for sign-in only, not to keep downloading or playing games
on the PS5 afterward. Keep this helper folder on your computer, not on the PS5.

### Windows PC

Windows sign-in is **not yet verified end to end**; these are the Windows steps
for running the supplied Node-based helper.

1. Install [Node.js 24 or newer for Windows](https://nodejs.org/en/download)
   and [Google Chrome](https://www.google.com/chrome/). Open a new terminal
   after installing Node so the `node` command is available.
2. Download the helper ZIP and choose **Extract All** in File Explorer. Open
   the extracted folder that contains **`server.mjs`**. Do not run it from
   inside the ZIP.
3. In that folder, click File Explorer's address bar, type **`powershell`**
   and press Enter. In the PowerShell window, run:

   ```powershell
   node --version
   node .\server.mjs
   ```

   The version must be **v24 or newer**. Leave the window open when it prints
   `XoDen sign-in: http://127.0.0.1:47832`.
4. On **that Windows PC**, open
   [http://127.0.0.1:47832](http://127.0.0.1:47832) in your browser, then follow
   **Pair the helper with your PS5** below. The local page may use Edge or
   another browser; Microsoft approval opens in a separate Chrome window.

### macOS

Install [Node.js 24 or newer](https://nodejs.org/en/download) and
[Google Chrome](https://www.google.com/chrome/). Extract the helper ZIP and
double-click **`start.command`**. Alternatively, open Terminal in the folder
containing `server.mjs` and run:

```sh
node server.mjs --open
```

Keep Terminal open. Open [http://127.0.0.1:47832](http://127.0.0.1:47832) on
that Mac if it did not open automatically, then follow the pairing steps below.

### Pair the helper with your PS5

1. In XoDen, open **Game Pass account** or **Settings → Xbox account →
   Sign in with Microsoft**. The console shows a pairing code.
2. On the **computer running the helper**, open
   [http://127.0.0.1:47832](http://127.0.0.1:47832). Enter the PS5's local
   network address and its 16-character pairing code in the form. The PS5
   address belongs in this form; the browser address stays `127.0.0.1:47832`.
3. Complete Microsoft's approval in the separate temporary **Chrome** window.
4. Wait for XoDen to confirm sign-in and refresh the library. You can then
   close the helper or press **Ctrl+C** in its terminal. It is not needed to
   keep downloading or playing games; start it again if XoDen asks you to sign
   in again.

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

## Build from source and test

For a source checkout, build the browser adapter first. Release ZIP users do not
need this step. Offline tests use synthetic callbacks and a temporary local TLS fixture:

```sh
npm ci --ignore-scripts
npm run build
npm test
```

The browser adapter is derived from Xodus Nexus under MIT; see
`browser/NOTICE.md` and `browser/LICENSE`. Build output and node_modules are
ignored. No console account files belong in this directory.
