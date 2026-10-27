#!/usr/bin/env python3
"""Build (never deploy/run) the experimental native Xbox C ABI probe.

Requires an installed nightly toolchain with rust-src, LLVM 18 and a PS5 SDK.
All standard-library adaptations and build outputs stay in --work-dir.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sdk', type=Path, required=True)
    parser.add_argument('--llvm-config', type=Path, required=True)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--toolchain', default='nightly')
    parser.add_argument('--job-engine', action='store_true', help='link the full job lifecycle probe (never deploy)')
    args = parser.parse_args()
    crate = Path(__file__).resolve().parents[1]
    workspace = crate.parents[1]
    sdk, llvm, work = args.sdk.resolve(), args.llvm_config.resolve(), args.work_dir.resolve()
    source = Path(run(['rustc', '+' + args.toolchain, '--print', 'sysroot'], capture_output=True, text=True).stdout.strip())
    if work == source or source in work.parents or work in source.parents:
        raise RuntimeError('work directory must be separate from the installed toolchain')
    version = run(['rustc', '+' + args.toolchain, '--version'],capture_output=True,text=True).stdout.strip()
    sysroot = work / 'sysroot'
    library = sysroot / 'lib/rustlib/src/rust/library'
    library.parent.mkdir(parents=True, exist_ok=True)
    for item in (source / 'lib/rustlib').iterdir():
        if item.name == 'src': continue
        destination = sysroot / 'lib/rustlib' / item.name
        if not destination.exists(): destination.symlink_to(item, target_is_directory=item.is_dir())
    stamp = work / 'toolchain.json'
    if stamp.exists() and json.loads(stamp.read_text()) != {'version':version, 'source':str(source)}:
        raise RuntimeError('toolchain changed; choose a fresh work directory')
    if not library.exists(): shutil.copytree(source / 'lib/rustlib/src/rust/library', library, symlinks=True)
    unix = library / 'std/src/sys/fs/unix.rs'
    if not unix.resolve().is_relative_to(work):
        raise RuntimeError('adapted source escaped the isolated work directory')
    text = unix.read_text()
    original, replacement = 'd_ino: (*entry_ptr).d_fileno,', 'd_ino: (*entry_ptr).d_fileno as u64,'
    if text.count(original) == 1: unix.write_text(text.replace(original,replacement))
    elif text.count(replacement) != 1: raise RuntimeError('unexpected Rust std source; review the adaptation')
    owned = library / 'std/src/os/fd/owned.rs'
    if not owned.resolve().is_relative_to(work):
        raise RuntimeError('adapted fd source escaped the isolated work directory')
    text = owned.read_text()
    original = 'let fd = cvt(unsafe { libc::fcntl(self.as_raw_fd(), cmd, 3) })?;'
    replacement = """// PS5 native titles cannot fork/exec, and reject F_DUPFD_CLOEXEC.
        // Preserve CLOEXEC using the supported operations; close on any failure.
        let fd = match cvt(unsafe { libc::fcntl(self.as_raw_fd(), cmd, 3) }) {
            Ok(fd) => fd,
            Err(error) if cfg!(target_os = "freebsd") && error.raw_os_error() == Some(libc::EOPNOTSUPP) => {
                let fd = cvt(unsafe { libc::fcntl(self.as_raw_fd(), libc::F_DUPFD, 3) })?;
                if let Err(error) = cvt(unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) }) {
                    unsafe { libc::close(fd); }
                    return Err(error);
                }
                fd
            }
            Err(error) => return Err(error),
        };"""
    if text.count(original) == 1: owned.write_text(text.replace(original,replacement))
    elif text.count(replacement) != 1: raise RuntimeError('unexpected Rust fd source; review the adaptation')
    stamp.write_text(json.dumps({'version':version, 'source':str(source)})+'\n')
    rustc = work / 'rustc-ps5'
    rustc.write_text('#!/bin/sh\nexec '+shlex.quote(str(source/'bin/rustc'))+' --sysroot '+shlex.quote(str(sysroot))+' "$@"\n')
    rustc.chmod(0o755)
    adaptation = hashlib.sha256((unix.read_text() + owned.read_text()).encode()).hexdigest()[:16]
    env = dict(os.environ)
    bindir = run([llvm, '--bindir'],capture_output=True,text=True).stdout.strip()
    env.update(LLVM_CONFIG=str(llvm), PS5_PAYLOAD_SDK=str(sdk), PATH=bindir+os.pathsep+env.get('PATH',''),
        RUSTC=str(rustc), CARGO_TARGET_DIR=str(work/'target'),
        CC_x86_64_unknown_freebsd=str(sdk/'bin/prospero-clang'),
        AR_x86_64_unknown_freebsd=str(sdk/'bin/prospero-ar'),
        CFLAGS_x86_64_unknown_freebsd='-include '+shlex.quote(str(crate/'tools/entropy_decl.h')),
        RUSTFLAGS='-Ztls-model=emulated --cfg libc_unstable_freebsd_version="11" --cfg xodus_ps5_std_revision="'+adaptation+'"')
    run(['cargo','+'+args.toolchain,'build','--manifest-path',workspace/'Cargo.toml','-p','xodus-ps5-native',
         *(['--features','job-engine'] if args.job_engine else []),
         '--locked','-Zbuild-std=std,panic_abort','--target','x86_64-unknown-freebsd','--release'],env=env)
    output = work/('xodus-job-probe.elf' if args.job_engine else 'xodus-header-probe.elf')
    run([sdk/'bin/prospero-clang','-Wl,--gc-sections',*(['-DXODUS_JOB_ENGINE'] if args.job_engine else []),'-I',crate/'include',crate/'tests/c_api.c',
         crate/'tools/probe_compat.c',work/'target/x86_64-unknown-freebsd/release/libxodus_ps5_native.a',
         *([crate/'tools/ps5_entropy.c','-Wl,--wrap=arc4random_buf'] if args.job_engine else []),
         '-lunwind','-o',output],env=env)
    print('Built only; not deployed or executed:',output)

if __name__ == '__main__': main()
