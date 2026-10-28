# Contributing to XoDen

XoDen support and compatibility reports belong in this repository. Please do not
send this fork's issues to the Xodus, Nexus or Playden teams.

## Easiest option: report a result

[Open the compatibility report form](https://github.com/griches/xoden_ps5/issues/new?template=compatibility-report.yml),
enter the game and what worked or failed, and submit it. A maintainer reviews
the report and updates the chart. You do not need to edit files, install Git,
or know Markdown. A GitHub account and access to this repository are required.
Reports do not automatically change a verdict.

## Propose a compatibility-list change

1. Read [COMPATIBILITY.md](COMPATIBILITY.md) and its evidence definitions.
2. [Open the list in GitHub’s editor](https://github.com/griches/xoden_ps5/edit/main/COMPATIBILITY.md),
   or open `COMPATIBILITY.md` and choose **Edit** (the pencil).
3. Make the change on a new branch, or in a fork if GitHub offers that workflow.
   Add the exact PC Game Pass product/edition, app and kit versions, firmware,
   graphics/library settings and what you actually tested.
4. Choose **Propose changes** and open a pull request explaining the evidence.
   A maintainer reviews and merges it into the list.

A successful download is not a successful launch. Use **Launches** for startup
or menus; **Works** requires a playable section. For failures, record the error
and the last successful step. Do not transfer a Steam edition's verdict without
testing the PC Game Pass edition. Preserve other people's test limitations.

If you prefer not to edit Markdown, use the
[compatibility report form](https://github.com/griches/xoden_ps5/issues/new?template=compatibility-report.yml).
A maintainer can update the chart from that report. Reports include successful
games as well as failures. In-app ratings remain local and do not submit to GitHub.

The repository is currently private. Invited collaborators can view and
contribute; people without access cannot download this release, open its Issues
or propose changes here. Making the repository public is a separate owner decision.

## Code and documentation

See [DEVELOPMENT.md](DEVELOPMENT.md) for source layout, build steps and licences.
Open a branch and a focused pull request. Describe the behavior changed, tests
run and console testing actually performed. Never claim a host test proves a
game works on PS5. Leave version tags and runtime-kit releases to the maintainer.

Do not include account sessions, tokens, device keys, pairing codes, signed
package URLs, private console configuration, game downloads or deployment backups.
Redact identifying data from screenshots/logs. Component and third-party licences
must stay with their source and distribution assets.
