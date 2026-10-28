# Changelog

## Public repository and README badges — 2026-10-28

- Opened the repository and release downloads to the public.
- Added stars, release and licence badges matching the other PS5 project READMEs.
- Added GitHub topics and updated the public compatibility-reporting workflow.

## Documentation and compatibility update — 2026-10-28

- Added explicit Windows PC setup and PowerShell commands to both sign-in READMEs.
  Windows end-to-end sign-in remains unverified; macOS remains the tested platform.
- Recorded Gary's working reports for Call of the Elder Gods, Donut County,
  DOOM 64, Hollow Knight and MIO: Memories in Orbit.
- Removed DREDGE from the current compatibility list at Gary's request.
- Refreshed release documentation and checksums; the PS5 executable is unchanged.

## 0.1.0 — 2026-10-28

First XoDen early-beta release. Git tag: `0.1`.

- Dedicated XoDen identity and PS5 title PPSA99324.
- Microsoft browser sign-in through the separate computer helper; clearer local-page instructions.
- PC Game Pass catalogue, search, package/size selection and native console downloads.
- Download pause/resume, retry/recovery work and smoothed speed/ETA display.
- Automatic per-game Windows setup and PS5 Home registration.
- Game Pass installs included in the storage inventory; remaining console checks tracked in the roadmap.
- Full-frame game backgrounds for generated Home artwork, with upgrades for older generated artwork.
- Automatic Microsoft shader-compiler dependency for the verified LIMBO Game Pass build.
- Specific Store device-limit guidance instead of a generic download failure for that licensing rejection.
- Separate checksum-pinned runtime-kit mirror, bundled Xbox runtime modules and retained upstream licenses/source attribution.

Gameplay is confirmed for several titles, including LIMBO, but compatibility is
limited. Halo: Campaign Evolved, High on Life 2, Balatro and BLiNX have recorded
startup/runtime failures. See the current COMPATIBILITY.md for later working
reports and edition/settings/test details.

Registration can need a return to PS5 Home, long-term sign-in renewal needs more
testing, some notifications truncate text, and other issues remain in ROADMAP.md.
App updates are manual in this beta. The first release was initially private;
the repository and release downloads are now public.
