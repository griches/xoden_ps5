# PS5-hosted Xodus backend prototype

This Windows x64 worker is intended to run inside the console's existing
Playden/Wine runtime. It is a backend prototype, not a native Playden Xbox UI,
and has now been executed on the console. No Mac relay is used by the worker.

`--ps5-worker` explicitly uses the existing private console account file at
`Z:\data\playden\xbox\xodus-keyring.ron` and redirects stdout/stderr to
`worker-progress.log` and `worker-errors.log` in that same private directory.
Provision the directory and log files as the Playden user, mode 0700/0600,
before running it. Never serve these logs publicly: account/profile commands
can contain account metadata, and diagnostics may include remote responses.
The worker does not perform interactive sign-in; it reuses the existing session.

Suggested acceptance sequence (one console job at a time):

1. Register this executable as a separate Imported utility in Playden and prepare
   its private bottle. Do not replace a working game's executable or profile.
2. Set its program arguments to `--ps5-worker account --json`. Launch, let it
   exit, then inspect only the signed-in boolean from the private output.
3. Test `--ps5-worker profile --json` and `--ps5-worker library --json --market GB`.
4. After verifying subscription and per-game access, test the selected small
   package with the streaming command below, to a NEW empty console directory.
5. Run prepare-ps5 on that directory, verify its executable, then register the
   downloaded game with Playden and finalize its Xbox runtime after preparation.

Example direct-console download arguments:

```text
--ps5-worker streaming 9N5PKH0KW2L1 Z:\data\xodus-downloads\minishoot --selected-content-id 65865037-5bc6-41ff-af08-71df860258cf --market GB --progress-json
```

Build in Lima from xodus-ps5-backend with the x86_64-pc-windows-gnu target,
`-p xodus-cli --no-default-features`. EAPPX extraction is optional and omitted
from this MSIXVC worker, avoiding its unrelated OpenSSL cross dependency.
Default Mac builds retain EAPPX extraction and native webview support.

Console acceptance: direct download and preparation of Minishoot completed, and
the downloaded copy reached its signed-in main menu. The structured `ps5-job`
refresh also completed on-console with 669 games and an active subscription.

`--ps5-worker ps5-job` reads a bounded `job-request.json` from the private Xbox
directory. Schema 1 requests use a UUID `requestId`, `operation` (`refresh` or
`install`), and two-letter `market`. Installs also require the catalog
`accountKey` and a 12-character `productId`; `contentId` is optional. Requests
cannot specify paths or URLs. Results are atomically published in
`job-result.json`; refresh publishes the full sanitized `catalog.json`.
An install rechecks live membership and entitlement, then downloads and prepares
the package. A `prepared` result still requires native Playden runtime setup
and title publication before it can be called installed.

Still required: controller job handoff/progress UI, on-console sign-in UX,
automatic per-game runtime finalization and end-to-end native UI acceptance. Do not describe this binary
as an independently working PS5 store until those checks pass.
