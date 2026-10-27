import argparse
import json
import unittest
from unittest.mock import patch
import ps5_xbox as xbox

PRODUCT = '9NP1P1WFS0LB'

class FakeConsole:
    def __init__(self):
        self.records = {
            'job-request.json': dict(schema=1, requestId='test', operation='install', productId=PRODUCT),
            'job-result.json': dict(schema=1, requestId='test', status='downloading', productId=PRODUCT),
            'prepared-games.json': dict(schema=1, games=[]),
        }
        self.removed = False
        self.writes = []
        self.ftp = self
    def close(self): pass
    def get(self, path): return json.dumps(self.records[path.rsplit('/', 1)[-1]]).encode()
    def put(self, data, name): self.writes.append(json.loads(data))
    def delete(self, name): pass
    def uid(self, commands):
        command = commands[0][0]
        if command == 'ls':
            return 'opendir: No such file or directory' if self.removed else '.nexus-streaming-v1.jsonl'
        if command == 'rm-xbox-partial':
            self.removed = True
            return 'removed-tree bytes=123'
        return 'published result'

class DiscardTests(unittest.TestCase):
    def run_discard(self, console, status='ok running title=FAKE77031'):
        with patch.object(xbox, 'Console', return_value=console), patch.object(xbox, 'running', return_value=status):
            xbox.discard_partial(argparse.Namespace(host='unused', product=PRODUCT))
    def test_current_partial_publishes_cancelled(self):
        c = FakeConsole()
        self.run_discard(c)
        self.assertTrue(c.removed)
        self.assertEqual(c.writes[0]['status'], 'cancelled')
    def test_orphan_preserves_newer_job(self):
        c = FakeConsole()
        c.records['job-request.json']['productId'] = '9MXND4PQLK3W'
        c.records['job-result.json']['productId'] = '9MXND4PQLK3W'
        self.run_discard(c)
        self.assertTrue(c.removed)
        self.assertEqual(c.writes, [])
    def test_prepared_game_refused(self):
        c = FakeConsole()
        c.records['prepared-games.json']['games'] = [{'productId': PRODUCT}]
        with self.assertRaises(RuntimeError): self.run_discard(c)
        self.assertFalse(c.removed)
    def test_worker_or_unknown_status_refused(self):
        for status in ['unknown', 'ok running title=FAKE78001']:
            c = FakeConsole()
            with self.assertRaises(RuntimeError): self.run_discard(c, status)
            self.assertFalse(c.removed)

if __name__ == '__main__': unittest.main()
