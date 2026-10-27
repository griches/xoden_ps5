import test from 'node:test';
import assert from 'node:assert/strict';
import { pcSubscription, command } from './backend.mjs';
import { parseEntitlements, entitlementKind, installSelection } from './nexus-loader.mjs';

const now = Date.parse('2026-10-26T18:00:00Z');
test('failed commands expose only structured transfer diagnostics', async () => {
  await assert.rejects(command(['-e', "console.error('private remote response');console.error('NEXUS_TRANSFER_DIAGNOSTIC_V1 metadata-invalid 0');process.exit(1)"], { executable: process.execPath }),
    { message: 'Package processing failed (metadata-invalid). Download files have been kept.' });
});
test('disk exhaustion reports required space without exposing local paths', async () => {
  await assert.rejects(command(['-e', "console.error('not enough free disk space on /private/location: need 107374182400 bytes, have 26843545600 bytes (files: 107374182400)');process.exit(1)"], { executable: process.execPath }),
    { message: 'Not enough space on the Mac: this download needs 100.0 GiB free; 25.0 GiB is available. Download files have been kept.' });
});
const account = 'fixture@example.test', market = 'GB', game = '9MW9469V91LM', plan = 'CFQ7TTC0KGQ8';
const row = (id, parent) => ({ productId: id, status: 'Active', isTrial: false, sharingSource: 'None',
  endDate: '2099-01-01T00:00:00Z', special: false, satisfiedByProductId: parent });
const response = entries => JSON.stringify({ schema: 1, account, market, checkedAt: new Date(now).toISOString(), entitlements: entries });

test('PC subscription requires a fresh active PC/Ultimate membership', () => {
  const profile = { subscription: { status: 'active', productIds: [plan], checkedAt: new Date(now).toISOString() } };
  assert.equal(pcSubscription(profile, now), true);
  assert.equal(pcSubscription(profile, now + 600000), false);
  profile.subscription.productIds = ['CFQ7TTC0K5DJ'];
  assert.equal(pcSubscription(profile, now), false);
  profile.subscription.status = 'unknown';
  assert.equal(pcSubscription(profile, now), false);
  assert.equal(pcSubscription(undefined, now), false);
});

test('Nexus rejects mismatched accounts, stale results, revoked subscriptions and cycles', () => {
  let parsed = parseEntitlements(response([row(game, plan), row(plan)]), account, market, now);
  assert.equal(entitlementKind(game, parsed.entries, now), 'gamepass');
  assert.throws(() => parseEntitlements(response([]), 'another@example.test', market, now));
  assert.throws(() => parseEntitlements(response([]), account, market, now + 300000));
  const revoked = { ...row(plan), status: 'Expired' };
  parsed = parseEntitlements(response([row(game, plan), revoked]), account, market, now);
  assert.equal(entitlementKind(game, parsed.entries, now), undefined);
  parsed = parseEntitlements(response([row(game, plan), row(plan, game)]), account, market, now);
  assert.equal(entitlementKind(game, parsed.entries, now), undefined);
});

test('Unknown or duplicate license fields never grant access', () => {
  const invalid = { ...row(game, plan) }; delete invalid.status;
  const parsed = parseEntitlements(response([row(game, plan), invalid]), account, market, now);
  assert.equal(entitlementKind(game, parsed.entries, now), undefined);
});

test('Package selection refuses arbitrary content IDs', () => {
  const content = '11111111-2222-3333-4444-555555555555';
  const input = { status: 'ready', choices: [{ contentId: content, supported: true }] };
  assert.equal(installSelection(input), content);
  assert.throws(() => installSelection(input, 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee'));
});
