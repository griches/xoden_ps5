#!/usr/bin/env python3
"""Fetch the pinned Wine runtime DLLs required by a fresh Game Pass install."""
import hashlib
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def fetch_shader_compiler(pin, output):
    if all((output / name).is_file() and not (output / name).is_symlink()
           and digest(output / name) == entry["sha256"] for name, entry in pin["files"].items()):
        return
    with tempfile.TemporaryDirectory(prefix="xoden-directx-") as temporary:
        archive = Path(temporary) / "directx.nupkg"
        print("Downloading pinned Microsoft shader compiler", flush=True)
        with urllib.request.urlopen(pin["archive_url"], timeout=60) as source, archive.open("wb") as target:
            shutil.copyfileobj(source, target)
        if digest(archive) != pin["archive_sha256"]:
            raise RuntimeError("Microsoft DirectX package checksum mismatch")
        with zipfile.ZipFile(archive) as package:
            for name, entry in pin["files"].items():
                data = package.read(entry["member"])
                if hashlib.sha256(data).hexdigest() != entry["sha256"]:
                    raise RuntimeError("Microsoft DirectX file checksum mismatch: " + name)
                staged = Path(temporary) / name
                staged.write_bytes(data)
                shutil.copyfile(staged, output / (name + ".tmp"))
                (output / (name + ".tmp")).replace(output / name)
    print("Pinned Microsoft shader compiler and licence verified")


def main():
    lock = json.loads((ROOT / "dependencies.lock.json").read_text())
    pin = lock["xbox_runtime"]
    output = ROOT / "deps/xbox-runtime"
    output.mkdir(parents=True, exist_ok=True)
    fetch_shader_compiler(lock["directx_shader_compiler"], output)
    if all((output / name).is_file() and digest(output / name) == sha
           for name, sha in pin["files"].items()):
        print("Pinned Xbox runtime DLLs are ready")
        return
    with tempfile.TemporaryDirectory(prefix="xoden-runtime-") as temporary:
        archive = Path(temporary) / "runtime.tar.xz"
        print("Downloading pinned Xodus Nexus Wine runtime", flush=True)
        with urllib.request.urlopen(pin["archive_url"], timeout=60) as source, archive.open("wb") as target:
            shutil.copyfileobj(source, target)
        if digest(archive) != pin["archive_sha256"]:
            raise RuntimeError("Xbox runtime archive checksum mismatch")
        with tarfile.open(archive) as package:
            for name, expected in pin["files"].items():
                # Upstream calls the implementation xgameruntime.dll; XoDen's
                # own wrapper uses that name, forwarding to the renamed DLL.
                original = "xgameruntime.dll" if name == "xgameruntime_impl.dll" else name
                matches = [member for member in package.getmembers()
                           if member.isfile() and Path(member.name).name == original
                           and "x86_64-windows" in member.name.split("/")]
                if len(matches) != 1:
                    raise RuntimeError("Expected one x86_64 runtime module: " + original)
                data = package.extractfile(matches[0]).read()
                if hashlib.sha256(data).hexdigest() != expected:
                    raise RuntimeError("Xbox runtime module checksum mismatch: " + name)
                staged = output / (name + ".tmp")
                staged.write_bytes(data)
                staged.replace(output / name)
    print("Pinned Xbox runtime DLLs verified")


if __name__ == "__main__":
    main()
