#!/usr/bin/env python3
"""Publish a completed Xbox PC download into Playden's existing local-import format.

The companion authorizes the account and downloads the selected package first.
This transport never decides whether an account owns a game.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import threading
import ftplib
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import sys
import time
import uuid
import xml.etree.ElementTree as ET

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import ps5_xbox as xbox

IMPORTS = '/data/playden/runtime-state/imports.json'
SETTINGS = '/data/playden/user/game-settings-private/game-settings.json'


def emit(stage, **values):
    print(json.dumps({'stage': stage, **values}), flush=True)


def safe_relative(value):
    value = value.replace('\\', '/')
    parts = value.split('/')
    if not value or value.startswith('/') or any(p in ('', '.', '..') for p in parts):
        raise ValueError('The game manifest contains an unsafe executable path.')
    if any(ord(c) < 32 or ord(c) > 126 for c in value) or ':' in value:
        raise ValueError('The executable path is not supported by Playden.')
    return value


def executable(root):
    configs = [p for p in root.iterdir() if p.name.lower() == 'microsoftgame.config']
    if len(configs) != 1 or configs[0].stat().st_size > 1024 * 1024:
        raise ValueError('A single MicrosoftGame.config is required for automatic setup.')
    tree = ET.fromstring(configs[0].read_bytes())
    candidates = []
    for node in tree.iter():
        if node.tag.rsplit('}', 1)[-1] == 'Executable' and node.attrib.get('Name'):
            name = safe_relative(node.attrib['Name'])
            if name.lower().endswith('.exe') and node.attrib.get('IsDevOnly', 'false').lower() != 'true':
                candidates.append(name)
    candidates = list(dict.fromkeys(candidates))
    if len(candidates) != 1:
        raise ValueError('This game needs an explicit executable choice; automatic installation is not available yet.')
    path = root / candidates[0]
    if not path.is_file() or path.is_symlink():
        raise ValueError('The main executable is missing.')
    with path.open('rb') as file:
        header = file.read(64)
        if len(header) < 64 or header[:2] != b'MZ':
            raise ValueError('The main executable is not a ready Windows PE image.')
        offset = int.from_bytes(header[60:64], 'little')
        if offset > 16 * 1024 * 1024:
            raise ValueError('Invalid PE header offset.')
        file.seek(offset)
        if file.read(6) != b'PE\0\0\x64\x86':
            raise ValueError('The current Xbox runtime requires a 64-bit Windows game.')
    return candidates[0]


def merged_records(imports, settings, product, title, program, now):
    if imports.get('version') != 1 or not isinstance(imports.get('games'), list) or not isinstance(imports.get('removed'), list):
        raise ValueError('Playden imports have an unsupported format.')
    if settings.get('version') != 1 or not isinstance(settings.get('games'), dict):
        raise ValueError('Playden settings have an unsupported format.')
    folder = 'playden-import/Xbox-' + product
    if any(game.get('folder') == folder and game.get('volume') == 'internal' for game in imports['games']):
        raise ValueError('This Xbox game is already registered with Playden.')
    # New identity only; existing imports and settings are left byte-for-byte equivalent as JSON values.
    identifier = str(uuid.uuid4())
    imports = json.loads(json.dumps(imports))
    settings = json.loads(json.dumps(settings))
    imports['games'].append({'id': identifier, 'title': title, 'volume': 'internal', 'folder': folder,
                            'program': program, 'state': 'in_place', 'added_at': now})
    settings['games']['local-' + identifier] = {'preset': '', 'revision': 1,
        'overrides': {'program': program, 'library_overrides': 'windows.web=n;xgameruntime=n'}}
    return imports, settings, identifier


def require_idle(host):
    if xbox.running(host) != 'ok idle':
        raise RuntimeError('Close Playden and any running game before installing. Downloaded files are kept for retry.')


def read_json(console, path):
    return json.loads(console.get(path))


def publish_json(console, path, data, name):
    content = json.dumps(data, ensure_ascii=True, separators=(',', ':')).encode()
    console.put(content, name)
    try:
        console.uid([('cp-private', f'{xbox.DROP}/{name}', path)])
        if console.get(path) != content:
            raise RuntimeError('Playden metadata verification failed; installation was not confirmed.')
    finally:
        console.delete(name)


def publish(args):
    require_idle(args.host)
    root = Path(args.source).resolve(strict=True)
    program = executable(root)
    files = []
    for path in sorted(root.rglob('*')):
        relative = path.relative_to(root)
        if any(part.startswith('.xodus') or part.startswith('.ps5-') for part in relative.parts):
            continue
        if path.is_symlink():
            raise ValueError('Symbolic links are not supported in game packages.')
        if path.is_file():
            safe_relative(relative.as_posix())
            files.append((path, relative.as_posix()))
    if not files:
        raise ValueError('The game package is empty.')
    console = xbox.Console(args.host)
    final = '/data/playden-import/Xbox-' + args.product
    stage = final + '.partial'
    try:
        # Require the existing shared account; do not read/export tokens during publishing.
        listing = console.uid([('ls', xbox.XBOX)])
        if not any(line.split()[-1:] == ['xodus-keyring.ron'] for line in listing.splitlines()):
            raise RuntimeError('Run ps5_xbox.py signin before installing the first game.')
        imports = read_json(console, IMPORTS)
        settings = read_json(console, SETTINGS)
        next_imports, next_settings, identifier = merged_records(imports, settings, args.product, args.title, program, int(time.time()))
        try:
            console.ftp.cwd(final)
        except ftplib.error_perm:
            pass
        else:
            raise RuntimeError('The game folder already exists; refusing to overwrite it.')
        console.ftp.cwd('/')
        for directory in ['/data/playden-import', stage]:
            try:
                console.ftp.mkd(directory)
            except ftplib.error_perm:
                console.ftp.cwd(directory)
                console.ftp.cwd('/')
        total = sum(path.stat().st_size for path, _ in files)
        done = 0
        created = {stage}
        # Create directories once, then use independent FTP sessions for file IO.
        for _, relative in files:
            parent = PurePosixPath(stage + '/' + relative).parent
            chain = []
            while str(parent) not in created:
                chain.append(str(parent)); parent = parent.parent
            for directory in reversed(chain):
                try:
                    console.ftp.mkd(directory)
                except ftplib.error_perm:
                    console.ftp.cwd(directory); console.ftp.cwd('/')
                created.add(directory)
        sessions = []
        sessions_lock = threading.Lock()
        worker = threading.local()

        def transfer(item):
            path, relative = item
            if not hasattr(worker, 'ftp'):
                ftp = ftplib.FTP()
                ftp.connect(args.host, 2121, timeout=60)
                ftp.login(); ftp.voidcmd('TYPE I')
                worker.ftp = ftp
                with sessions_lock:
                    sessions.append(ftp)
            ftp = worker.ftp
            remote = stage + '/' + relative
            matched = False
            if args.resume:
                try:
                    if ftp.size(remote) == path.stat().st_size:
                        remote_hash = hashlib.sha256()
                        ftp.retrbinary('RETR ' + remote, remote_hash.update, blocksize=256 * 1024)
                        with path.open('rb') as stream:
                            matched = remote_hash.digest() == hashlib.file_digest(stream, 'sha256').digest()
                except ftplib.error_perm:
                    pass
            if not matched:
                with path.open('rb') as stream:
                    ftp.storbinary('STOR ' + remote, stream, blocksize=256 * 1024)
            if ftp.size(remote) != path.stat().st_size:
                raise RuntimeError('A game file did not upload completely.')
            return path.stat().st_size

        pool = ThreadPoolExecutor(max_workers=4)
        futures = [pool.submit(transfer, item) for item in files]
        last_progress = 0
        try:
            for future in as_completed(futures):
                done += future.result()
                if time.monotonic() - last_progress >= 1 or done == total:
                    emit('uploading', done=done, total=total)
                    last_progress = time.monotonic()
        finally:
            pool.shutdown(wait=True, cancel_futures=True)
            for ftp in sessions:
                ftp.close()
        # The metadata connection may have been idle through a large transfer.
        console.ftp.close()
        console.ftp = ftplib.FTP()
        console.ftp.connect(args.host, 2121, timeout=60)
        console.ftp.login()
        console.ftp.voidcmd('TYPE I')
        # The runtime sits beside the main executable, loaded by the per-game native overrides.
        remote_program_dir = str(PurePosixPath(stage + '/' + program).parent)
        for name, local in xbox.RUNTIME.items():
            content = xbox.runtime_bytes(name, local)
            remote = remote_program_dir + '/' + name
            console.ftp.storbinary('STOR ' + remote, io.BytesIO(content))
            check = io.BytesIO()
            console.ftp.retrbinary('RETR ' + remote, check.write)
            if hashlib.sha256(check.getvalue()).digest() != hashlib.sha256(content).digest():
                raise RuntimeError('Xbox runtime upload did not verify.')
        require_idle(args.host)
        # Re-read before writing: never replace settings changed during the upload.
        if read_json(console, IMPORTS) != imports or read_json(console, SETTINGS) != settings:
            raise RuntimeError('Playden configuration changed during upload. Close it and retry.')
        console.ftp.rename(stage, final)
        # Settings first: failure leaves no launchable import with missing overrides.
        publish_json(console, SETTINGS, next_settings, 'xbox-settings.json')
        publish_json(console, IMPORTS, next_imports, 'xbox-imports.json')
        emit('installed', installId='local-' + identifier, message='Open Playden and select the game to prepare its PS5 title.')
    finally:
        console.ftp.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--product', required=True)
    parser.add_argument('--title', required=True)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--resume', action='store_true', help='Hash-check existing staged files and reuse matching uploads')
    parser.add_argument('--preflight', action='store_true', help='Check console availability and refuse an already transferred game')
    args = parser.parse_args()
    if not re.fullmatch('[A-Z0-9]{12}', args.product) or not args.title or len(args.title.encode()) > 512:
        parser.error('Invalid product or title.')
    if args.preflight:
        require_idle(args.host)
        console = xbox.Console(args.host)
        try:
            try:
                console.ftp.cwd('/data/playden-import/Xbox-' + args.product)
            except ftplib.error_perm:
                emit('ready')
            else:
                raise RuntimeError('This game has already been transferred. Open it in Playden; automatic reinstall is not supported.')
        finally:
            console.ftp.close()
    elif args.check:
        emit('checked', executable=executable(Path(args.source).resolve(strict=True)))
    else:
        publish(args)


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        emit('failed', message=str(error))
        sys.exit(1)
