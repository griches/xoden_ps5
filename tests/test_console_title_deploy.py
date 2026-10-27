# SPDX-License-Identifier: GPL-3.0-or-later
import ftplib
import sys
import unittest
from pathlib import Path
from unittest.mock import Mock
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import console_title_deploy as deploy

class RawTransferTests(unittest.TestCase):
    def test_self_server_requires_disabled_reply(self):
        ftp = Mock()
        ftp.sendcmd.side_effect = ['211-Features:\n SELF\n211 End', '226 SELF transfer mode disabled']
        deploy.configure_raw_transfers(ftp)
        self.assertEqual([c.args[0] for c in ftp.sendcmd.call_args_list], ['FEAT', 'SELF'])
    def test_unconfirmed_mode_fails_before_transfers(self):
        ftp = Mock()
        ftp.sendcmd.side_effect = ['211-Features:\n SELF\n211 End', '226 SELF transfer mode enabled']
        with self.assertRaises(RuntimeError): deploy.configure_raw_transfers(ftp)
        ftp.retrbinary.assert_not_called()
        ftp.storbinary.assert_not_called()
    def test_ordinary_server_is_not_toggled(self):
        ftp = Mock()
        ftp.sendcmd.return_value = '211-Features:\n MLSD\n211 End'
        deploy.configure_raw_transfers(ftp)
        ftp.sendcmd.assert_called_once_with('FEAT')
    def test_unsupported_feat_and_other_errors(self):
        ftp = Mock()
        ftp.sendcmd.side_effect = ftplib.error_perm('502 Unsupported')
        deploy.configure_raw_transfers(ftp)
        ftp.sendcmd.side_effect = ftplib.error_perm('530 Not logged in')
        with self.assertRaises(ftplib.error_perm): deploy.configure_raw_transfers(ftp)
    def test_delete_accepts_only_success(self):
        ftp = Mock()
        ftp.delete.side_effect = ftplib.error_reply('226 File deleted')
        deploy.delete_file(ftp, '/old')
        ftp.delete.side_effect = ftplib.error_reply('450 Busy')
        with self.assertRaises(ftplib.error_reply): deploy.delete_file(ftp, '/old')

if __name__ == '__main__': unittest.main()
