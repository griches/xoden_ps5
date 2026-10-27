import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch
import install_ps5

xbox = install_ps5.xbox


class SigninTests(unittest.TestCase):
    def test_running_game_prevents_keychain_read_and_publication(self):
        with patch.object(xbox, 'running', return_value='ok running title=fixture'), \
                patch.object(xbox.subprocess, 'run') as command, \
                patch.object(xbox, 'Console') as console:
            with self.assertRaisesRegex(RuntimeError, 'Close Game Pass'):
                xbox.signin(SimpleNamespace(host='fixture', no_login=True))
            command.assert_not_called()
            console.assert_not_called()

    def exercise_transfer(self, stored):
        console = MagicMock()
        console.uid.return_value = f'published {xbox.XBOX}/xodus-keyring.ron'
        console.get.return_value = stored
        def export(args, **kwargs):
            Path(args[1]).write_bytes(b'synthetic-session')
        with patch.object(xbox, 'running', return_value='ok idle'), \
                patch.object(xbox.subprocess, 'run', side_effect=export), \
                patch.object(xbox, 'Console', return_value=console), \
                patch('builtins.print'):
            try:
                xbox.signin(SimpleNamespace(host='fixture', no_login=True))
            finally:
                staged = console.put.call_args.args[1]
                console.delete.assert_any_call(staged)
                console.delete.assert_any_call('out.bin')
                console.ftp.close.assert_called_once()
        return console

    def test_exact_session_is_verified_and_staging_is_removed(self):
        console = self.exercise_transfer(b'synthetic-session')
        self.assertEqual(console.put.call_args.args[0], b'synthetic-session')

    def test_incorrect_readback_fails_and_still_removes_staging(self):
        with self.assertRaisesRegex(RuntimeError, 'did not pass verification'):
            self.exercise_transfer(b'wrong-session')

    def test_title_started_during_export_prevents_console_mutation(self):
        def export(args, **kwargs):
            Path(args[1]).write_bytes(b'synthetic-session')
        with patch.object(xbox, 'running', side_effect=['ok idle', 'ok running title=fixture']), \
                patch.object(xbox.subprocess, 'run', side_effect=export), \
                patch.object(xbox, 'Console') as console:
            with self.assertRaisesRegex(RuntimeError, 'started a title'):
                xbox.signin(SimpleNamespace(host='fixture', no_login=True))
            console.assert_not_called()


if __name__ == '__main__':
    unittest.main()
