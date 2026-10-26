import copy
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
import ftplib
import install_ps5 as installer


class InstallTests(unittest.TestCase):
    def test_console_becoming_busy_prevents_metadata_publication(self):
        class MemoryFTP:
            def __init__(self):
                self.files = {}; self.directories = {'/'}; self.renamed = False
            def connect(self, *a, **k): pass
            def login(self): pass
            def voidcmd(self, value): pass
            def close(self): pass
            def cwd(self, path):
                if path not in self.directories: raise ftplib.error_perm('550 absent')
            def mkd(self, path): self.directories.add(path)
            def storbinary(self, command, stream, **kwargs): self.files[command[5:]] = stream.read()
            def retrbinary(self, command, callback, **kwargs): callback(self.files[command[5:]])
            def size(self, path):
                if path not in self.files: raise ftplib.error_perm('550 absent')
                return len(self.files[path])
            def rename(self, *a): self.renamed = True

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'MicrosoftGame.config').write_text('<Game><Executable Name="Game.exe"/></Game>')
            header = bytearray(70); header[:2] = b'MZ'; header[60:64] = (64).to_bytes(4, 'little'); header[64:70] = b'PE\0\0\x64\x86'
            (root / 'Game.exe').write_bytes(header)
            ftp = MemoryFTP()
            console = SimpleNamespace(ftp=ftp, uid=lambda commands: '- 0600 1 100 xodus-keyring.ron')
            args = SimpleNamespace(host='fixture', source=str(root), product='9MW9469V91LM', title='Fixture', resume=False)
            imports = {'version': 1, 'games': [], 'removed': []}
            settings = {'version': 1, 'games': {}}
            with patch.object(installer.xbox, 'Console', return_value=console), \
                 patch.object(installer.ftplib, 'FTP', return_value=ftp), \
                 patch.object(installer.xbox, 'RUNTIME', {'xgameruntime.dll': str(root / 'Game.exe')}), \
                 patch.object(installer, 'require_idle', side_effect=[None, RuntimeError('Console busy')]), \
                 patch.object(installer, 'read_json', side_effect=[imports, settings]), \
                 patch.object(installer, 'emit'), \
                 patch.object(installer, 'publish_json') as publish:
                with self.assertRaisesRegex(RuntimeError, 'Console busy'):
                    installer.publish(args)
                publish.assert_not_called()
                self.assertFalse(ftp.renamed)

    def test_preserves_existing_records_and_does_not_mutate_inputs(self):
        imports = {'version': 1, 'games': [{'id': 'existing', 'folder': 'old', 'volume': 'internal'}], 'removed': []}
        settings = {'version': 1, 'games': {'220': {'preset': 'old', 'revision': 5, 'overrides': {'graphics': 'dxvk'}}}}
        before = copy.deepcopy((imports, settings))
        added, configured, identifier = installer.merged_records(imports, settings, '9MW9469V91LM', 'Hollow Knight', 'Game.exe', 123)
        self.assertEqual((imports, settings), before)
        self.assertEqual(added['games'][0], imports['games'][0])
        self.assertEqual(configured['games']['220'], settings['games']['220'])
        self.assertEqual(configured['games']['local-' + identifier]['overrides']['program'], 'Game.exe')
        self.assertEqual(configured['games']['local-' + identifier]['overrides']['library_overrides'], 'windows.web=n;xgameruntime=n')
        with self.assertRaises(ValueError):
            installer.merged_records(added, configured, '9MW9469V91LM', 'Hollow Knight', 'Game.exe', 124)

    def test_path_traversal_and_control_characters_refused(self):
        for path in ('../Game.exe', 'C:\\Game.exe', '/Game.exe', 'a//Game.exe', 'a/./Game.exe', 'Game\n.exe'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                installer.safe_relative(path)

    def test_manifest_requires_one_ready_x64_program(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = root / 'MicrosoftGame.config'
            config.write_text('<Game><ExecutableList><Executable Name="Game.exe"/></ExecutableList></Game>')
            with self.assertRaises(ValueError): installer.executable(root)
            header = bytearray(70); header[:2] = b'MZ'; header[60:64] = (64).to_bytes(4, 'little'); header[64:70] = b'PE\0\0\x64\x86'
            (root / 'Game.exe').write_bytes(header)
            self.assertEqual(installer.executable(root), 'Game.exe')
            config.write_text('<Game><Executable Name="Game.exe"/><Executable Name="Other.exe"/></Game>')
            with self.assertRaises(ValueError): installer.executable(root)


if __name__ == '__main__':
    unittest.main()
