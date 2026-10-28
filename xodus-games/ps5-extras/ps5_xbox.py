#!/usr/bin/env python3
"""Xbox (Xodus) sign-in and per-game runtime setup for Playden on a PS5.

  ps5_xbox.py --host IP signin [--no-login]   sign in on this Mac, install it on the console
  ps5_xbox.py --host IP bottles               list Playden's games and their bottles
  ps5_xbox.py --host IP setup INSTALL_ID      Xbox runtime + wrapper + override into a bottle
  ps5_xbox.py --host IP doctor INSTALL_ID    verify runtime files, registry and sign-in presence
  ps5_xbox.py --host IP status                which title is running (ps5vkctl)

Files are placed as Playden's user (uid 1) by the uidcp payload, sent to the
console's ELF loader on port 9021. The console-wide sign-in lives at
/data/playden/xbox/xodus-keyring.ron; the wrapper in each game reads it.
"""
import argparse
import ftplib
import io
import json
from datetime import datetime, timezone
import os
import re
import hashlib
import socket
import subprocess
import sys
import tempfile
import time
import uuid

HERE = os.path.dirname(os.path.abspath(__file__))
UIDCP = os.path.join(HERE, "uidcp", "uidcp.elf")
XODUS_CLI = os.path.join(HERE, "xodus-cli")
XODUS_EXPORT = os.path.join(HERE, "xodus-export", "target", "release", "xodus-export")
RUNTIME = {  # bottle system32 name -> local file
    "xgameruntime.dll": os.path.join(HERE, "runtime", "xgameruntime.dll"),  # our wrapper
    "xgameruntime_impl.dll": os.path.join(HERE, "runtime", "xgameruntime_impl.dll"),  # Nexus runtime
    "windows.web.dll": os.path.join(HERE, "runtime", "windows.web.dll"),  # Xodus JSON fixes
}
DROP = "/data/xodus-drop"
GAMES = "/data/playden/games"
XBOX = "/data/playden/xbox"
OVERRIDE_SECTION = b"[Software\\\\Wine\\\\DllOverrides]"
OVERRIDE_LINE = b'"windows.web"="native"\n'


class Console:
    def __init__(self, host):
        self.host = host
        self.ftp = ftplib.FTP()
        self.ftp.connect(host, 2121, timeout=60)
        self.ftp.login()
        self.ftp.voidcmd("TYPE I")
        try:
            self.ftp.mkd(DROP)
        except ftplib.error_perm:
            pass
        self.ftp.sendcmd(f"SITE CHMOD 777 {DROP}")

    def delete(self, name):
        try:
            self.ftp.delete(f"{DROP}/{name}")
        except (ftplib.error_reply, ftplib.error_perm):  # ftpsrv answers DELE with 226
            pass

    def put(self, data, name):
        self.delete(name)  # a uid-1 file of that name can't be overwritten by ftpsrv
        self.ftp.storbinary(f"STOR {DROP}/{name}", io.BytesIO(data))
        self.ftp.sendcmd(f"SITE CHMOD 644 {DROP}/{name}")

    def fetch(self, name):
        buffer = io.BytesIO()
        self.ftp.retrbinary(f"RETR {DROP}/{name}", buffer.write)
        return buffer.getvalue()

    def uid(self, commands):
        """Runs uidcp commands (tuples) as Playden's user; returns the log text."""
        text = "".join("\t".join(c) + "\n" for c in commands)
        self.put(text.encode(), "uidcp.txt")
        self.delete("uidcp.log")
        with socket.create_connection((self.host, 9021), timeout=10) as s:
            with open(UIDCP, "rb") as payload:
                s.sendall(payload.read())
            s.shutdown(socket.SHUT_WR)
        log = b""
        for _ in range(60):
            time.sleep(1)
            try:
                log = self.fetch("uidcp.log")
            except ftplib.Error:
                continue
            if log.rstrip().endswith(b"done") or b"seteuid(1):" in log or b"no /data" in log:
                break
        return log.decode("utf-8", "replace")

    def get(self, path):
        """Copies a uid-1 file out through the drop folder and returns its bytes."""
        log = self.uid([("get", path, "out.bin")])
        if "bytes" not in log:
            raise RuntimeError(f"could not read {path}:\n{log}")
        data = self.fetch("out.bin")
        self.delete("out.bin")
        return data


def running(host):
    try:
        with socket.create_connection((host, 9111), timeout=8) as s:
            s.sendall(b"status\n")
            s.settimeout(4)
            return s.recv(300).decode().strip()
    except OSError as e:
        return f"unknown ({e})"


def signin(args):
    if running(args.host) != "ok idle":
        raise RuntimeError("Close Game Pass and any running game before saving sign-in.")
    if not args.no_login:
        print("Opening the Microsoft sign-in window (xodus-cli login)...")
        subprocess.run([XODUS_CLI, "login"], check=True)
    with tempfile.TemporaryDirectory() as tmp:
        os.chmod(tmp, 0o700)
        path = os.path.join(tmp, "xodus-keyring.ron")
        print("Reading the sign-in from the Keychain (allow xodus-export if asked)...")
        subprocess.run([XODUS_EXPORT, path], check=True)
        with open(path, "rb") as exported:
            data = exported.read()
    # Recheck after the Keychain dialog; never replace credentials while a
    # title may be refreshing the current account.
    if running(args.host) != "ok idle":
        raise RuntimeError("The PS5 started a title. Close it and retry saving sign-in.")
    console = Console(args.host)
    staged = f"signin-{uuid.uuid4().hex}.ron"
    try:
        console.put(data, staged)
        log = console.uid([("mkdir", XBOX), ("cp-private", f"{DROP}/{staged}", f"{XBOX}/xodus-keyring.ron")])
        if f"published {XBOX}/xodus-keyring.ron" not in log:
            raise RuntimeError("The PS5 could not save the Microsoft sign-in. Try again.")
        installed = console.get(f"{XBOX}/xodus-keyring.ron")
        if hashlib.sha256(installed).digest() != hashlib.sha256(data).digest():
            raise RuntimeError("The saved Microsoft session did not pass verification.")
    finally:
        console.delete(staged)
        console.delete("out.bin")
        console.ftp.close()
    print("Microsoft browser sign-in saved and verified on the PS5.")


def bottles(args):
    console = Console(args.host)
    log = console.uid([("ls", GAMES)])
    for line in log.splitlines():
        parts = line.split()
        if len(parts) < 5 or parts[0] != "d" or parts[-1].endswith(".kept"):
            continue
        install = parts[-1]
        name = "?"
        try:
            profile = console.get(f"{GAMES}/{install}/library/profiles/playden-{install}.profile").decode()
            name = next((l.split("=", 1)[1].strip() for l in profile.splitlines() if l.startswith("name =")), "?")
        except RuntimeError:
            pass
        print(f"{install:50} {name}")


def install_id(value):
    if not re.fullmatch(r"(?:local-[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}|steam-[1-9][0-9]*)", value):
        raise argparse.ArgumentTypeError("expected a Playden install id from 'bottles'")
    return value


def native_override(reg):
    """Replace windows.web only inside DllOverrides; preserve other sections."""
    lines = reg.splitlines(keepends=True)
    start = next((i for i, line in enumerate(lines)
                  if line.rstrip(b"\r\n").startswith(OVERRIDE_SECTION)), None)
    if start is None:
        return reg.rstrip(b"\r\n") + b"\n\n" + OVERRIDE_SECTION + b"\n" + OVERRIDE_LINE
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith(b"[")), len(lines))
    section = [line for line in lines[start:end]
               if not re.match(br'"windows\.web"\s*=', line)]
    if section and not section[-1].endswith(b"\n"):
        section[-1] += b"\n"
    section.append(OVERRIDE_LINE)
    return b"".join(lines[:start] + section + lines[end:])


def runtime_bytes(name, local):
    with open(local, "rb") as runtime:
        data = bytearray(runtime.read())
    if name == "windows.web.dll" and data[0x40:0x50] == b"Wine builtin DLL":
        data[0x40:0x50] = bytes(16)
    return bytes(data)


def doctor(args):
    """Inspect configuration without reading account tokens or launching a game."""
    print(f"Console: {running(args.host)}")
    prefix = f"{GAMES}/{args.install}/library/prefixes/game"
    console = Console(args.host)
    failures = []
    try:
        for name, local in RUNTIME.items():
            try:
                actual = console.get(f"{prefix}/drive_c/windows/system32/{name}")
                expected = runtime_bytes(name, local)
                ok = actual == expected
                print(f"{'OK' if ok else 'FAIL'} {name}: {len(actual)} bytes, sha256 {hashlib.sha256(actual).hexdigest()}")
                if not ok:
                    failures.append(name)
            except (RuntimeError, ftplib.Error) as error:
                print(f"FAIL {name}: {error}")
                failures.append(name)
        reg = console.get(f"{prefix}/user.reg")
        # Normalizing twice is stable; compare the actual value in the target section.
        section = reg.split(OVERRIDE_SECTION, 1)[-1].split(b"\n[", 1)[0] if OVERRIDE_SECTION in reg else b""
        values = re.findall(br'^"windows\.web"\s*=\s*"([^"\r\n]*)"', section, re.M)
        ok = values == [b"native"]
        print(f"{'OK' if ok else 'FAIL'} windows.web native override")
        if not ok:
            failures.append("registry override")
        listing = console.uid([("ls", XBOX)])
        present = any(line.split()[-1:] == ["xodus-keyring.ron"] for line in listing.splitlines())
        print(f"{'OK' if present else 'FAIL'} shared sign-in file present (token validity not checked)")
        if not present:
            failures.append("shared sign-in")
    finally:
        console.ftp.close()
    if failures:
        raise SystemExit("Configuration check failed: " + ", ".join(failures))
    print("Configuration verified; gameplay and live sign-in require a launch test.")


def setup(args):
    state = running(args.host)
    if "PPSA99324" not in state and "idle" not in state:
        sys.exit(f"close the game first (Wine rewrites the registry on exit): {state}")
    prefix = f"{GAMES}/{args.install}/library/prefixes/game"
    console = Console(args.host)
    reg = native_override(console.get(f"{prefix}/user.reg"))
    commands = []
    for name, local in RUNTIME.items():
        console.put(runtime_bytes(name, local), name)
        commands.append(("cp", f"{DROP}/{name}", f"{prefix}/drive_c/windows/system32/{name}"))
    console.put(reg, "user.reg")
    commands.append(("cp", f"{DROP}/user.reg", f"{prefix}/user.reg"))
    log = console.uid(commands)
    for name in list(RUNTIME) + ["user.reg"]:
        console.delete(name)
    print(log)
    console.ftp.close()
    doctor(args)


def discard_partial(args):
    """Discard an unprepared Xbox download, including older orphaned partials."""
    product = args.product.upper()
    if not re.fullmatch(r"[A-Z0-9]{12}", product):
        raise ValueError("invalid Xbox product id")
    status = running(args.host)
    # Fail closed when status is unavailable or the downloader is running.
    if not status.startswith("ok ") or "title=FAKE78001" in status:
        raise RuntimeError("Download worker must be stopped before discarding files")
    c = Console(args.host)
    try:
        request = json.loads(c.get(XBOX + "/job-request.json"))
        result = json.loads(c.get(XBOX + "/job-result.json"))
        prepared = json.loads(c.get(XBOX + "/prepared-games.json"))
        if (request.get("schema") != 1 or result.get("schema") != 1 or
                prepared.get("schema") != 1 or not isinstance(prepared.get("games"), list)):
            raise RuntimeError("Download records could not be validated")
        matching = (request.get("operation") == "install" and request.get("productId") == product and
                    result.get("productId") == product and bool(request.get("requestId")) and
                    request.get("requestId") == result.get("requestId"))
        if (matching and result.get("status") in ("prepared", "complete")) or any(
                row.get("productId", "").upper() == product for row in prepared["games"]):
            raise RuntimeError("Refusing to discard an installed/prepared game")
        path = "/data/playden-games/xbox-" + product.lower()
        listing = c.uid([("ls", path)])
        if not matching and ".nexus-streaming-v1.jsonl" not in listing:
            raise RuntimeError("Orphan folder has no download resume marker; refusing removal")
        if running(args.host) != status:
            raise RuntimeError("Console activity changed; retry when the worker is stopped")
        path = "/data/playden-games/xbox-" + product.lower()
        log = c.uid([("rm-xbox-partial", path)])
        match = re.search(r"removed-tree bytes=(\d+)", log)
        if not match:
            raise RuntimeError("Cleanup did not complete: " + log)
        if matching:
            # Publish terminal state only after successful deletion. Retrying is safe.
            result.update(status="cancelled", detail="Partial download deleted",
                          updatedAt=datetime.now(timezone.utc).isoformat())
            c.put(json.dumps(result).encode(), "cancelled-result.json")
            published = c.uid([("cp-private", DROP + "/cancelled-result.json", XBOX + "/job-result.json")])
            c.delete("cancelled-result.json")
            if "published " not in published:
                raise RuntimeError("Files removed but cancellation state could not be saved")
        check = c.uid([("ls", path)])
        if "opendir: No such file or directory" not in check:
            raise RuntimeError("Cleanup could not be verified: " + check)
        print(f"Deleted {product} partial download: {int(match.group(1)):,} bytes. Verified folder absent.")
    finally:
        c.ftp.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--host", default=os.environ.get("PS5_HOST"), required=os.environ.get("PS5_HOST") is None)
    sub = parser.add_subparsers(dest="command", required=True)
    s = sub.add_parser("signin")
    s.add_argument("--no-login", action="store_true", help="use the sign-in already in the Keychain")
    sub.add_parser("bottles")
    s = sub.add_parser("setup")
    s.add_argument("install", type=install_id, help="install id from 'bottles', e.g. local-529ec159-...")
    s = sub.add_parser("doctor")
    s.add_argument("install", type=install_id)
    sub.add_parser("status")
    s = sub.add_parser("discard-partial", help="delete an interrupted Xbox download; preserves completed games")
    s.add_argument("product")
    args = parser.parse_args()
    if args.command == "status":
        print(running(args.host))
    else:
        {"discard-partial": discard_partial, "signin": signin, "bottles": bottles, "setup": setup, "doctor": doctor}[args.command](args)


if __name__ == "__main__":
    main()
