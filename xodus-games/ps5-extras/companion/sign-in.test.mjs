import test from 'node:test';
import assert from 'node:assert/strict';
import { signIn } from './sign-in.mjs';
import { command } from './backend.mjs';

function fixture(statuses, login = async () => {}) {
  const updates = [], abort = new AbortController();
  let calls = 0, resets = 0;
  return { updates, abort, get calls() { return calls; }, get resets() { return resets; },
    run: () => signIn({ signal: abort.signal, report: update => updates.push(update),
      command: async (...args) => { calls++; return login(...args); },
      backend: { status: async () => { const value = statuses.shift(); if (value instanceof Error) throw value; return value; }, library: { reset() { resets++; } } } }) };
}
const valid = { signedIn: true, sessionVerified: true, pcGamePass: true };
test('verified saved account does not reopen login', async () => {
  const f = fixture([valid, valid]); await f.run();
  assert.equal(f.calls, 0); assert.equal(f.updates.at(-1).stage, 'signed-in');
});
test('a stored but unverified account requires login and verification', async () => {
  const f = fixture([{ signedIn: true }, valid]); await f.run();
  assert.equal(f.calls, 1); assert.equal(f.resets, 1);
});
test('a failed precheck still allows login', async () => {
  const f = fixture([Error('offline'), valid]); await f.run(); assert.equal(f.calls, 1);
});
test('missing subscription is distinct from failed sign-in', async () => {
  const noPlan = { ...valid, pcGamePass: false };
  const f = fixture([noPlan, noPlan]); await f.run();
  assert.equal(f.calls, 0); assert.match(f.updates.at(-1).message, /downloads remain unavailable/);
});
test('successful login process cannot falsely validate an expired session', async () => {
  const f = fixture([{}, { signedIn: true }]);
  await assert.rejects(f.run(), /could not be verified/);
  assert.ok(!f.updates.some(v => v.stage === 'signed-in'));
});
test('timeout and closed window have actionable messages without backend output', async () => {
  for (const [error, expected] of [['service timed out', /expired after ten minutes/], ['private backend detail', /window may have been closed/]]) {
    const f = fixture([{}], async () => { throw Error(error); });
    await assert.rejects(f.run(), expected);
  }
});
test('cancelling prevents late verification from claiming success', async () => {
  const f = fixture([{}], async () => f.abort.abort());
  await assert.rejects(f.run(), /cancelled/);
  assert.ok(!f.updates.some(v => v.stage === 'signed-in'));
});
test('cancel drains the child before returning and never exposes its output', async () => {
  const abort = new AbortController();
  const result = command(['-e', 'setInterval(()=>{},1000)'], { executable: process.execPath, signal: abort.signal });
  abort.abort(); await assert.rejects(result, /Sign-in cancelled/);
});
