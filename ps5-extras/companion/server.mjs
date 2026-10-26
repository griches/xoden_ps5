import { createServer } from 'node:http';
import { readFile, mkdir, writeFile, rename, statfs, stat } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { randomBytes } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { Backend, command, binary, market } from './backend.mjs';

const backend = new Backend();
const port = Number(process.env.XBOX_COMPANION_PORT ?? 47831);
const origin = `http://127.0.0.1:${port}`;
const token = randomBytes(32).toString('hex');
let config = {};
try { config = JSON.parse(await readFile(new URL('config.json', import.meta.url), 'utf8')); }
catch (error) { if (error.code !== 'ENOENT') throw error; }
const downloadRoot = resolve(process.env.XBOX_DOWNLOAD_ROOT ?? config.downloadRoot ?? fileURLToPath(new URL('../../downloads/', import.meta.url)));
const host = process.env.PS5_HOST ?? config.ps5Host ?? '192.168.0.174';
process.env.PS5_HOST = host;
const html = (await readFile(new URL('index.html', import.meta.url), 'utf8')).replace('__TOKEN__', token);
let job = { stage: 'idle' };
const lastJobPath = new URL('.last-job.json', import.meta.url);
try {
  const previous = JSON.parse(await readFile(lastJobPath, 'utf8'));
  if (['failed', 'installed', 'signed-in'].includes(previous.stage)) job = previous;
} catch { /* No completed task yet. */ }
let busy = false;

function send(res, code, value) {
  res.writeHead(code, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store',
    'X-Content-Type-Options': 'nosniff', 'Referrer-Policy': 'no-referrer' });
  res.end(JSON.stringify(value));
}

async function body(req) {
  let text = '';
  for await (const chunk of req) {
    text += chunk;
    if (text.length > 8192) throw Error('Request too large.');
  }
  return JSON.parse(text || '{}');
}

function launch(action) {
  if (busy) throw Error('An account or installation task is already running.');
  busy = true;
  job = { stage: 'checking' };
  action().catch(error => {
    const message = job.stage === 'failed' && job.message ? job.message : error.message;
    job = { ...job, stage: 'failed', message };
  })
    .finally(async () => {
      busy = false;
      try { await writeFile(lastJobPath, JSON.stringify({ ...job, busy: false }), { mode: 0o600 }); }
      catch { /* Status remains available in memory. */ }
    });
}

async function install(productId, contentId) {
  const { choice, account } = await backend.authorize(productId, contentId);
  const xboxHelper = fileURLToPath(new URL('../ps5_xbox.py', import.meta.url));
  const state = await command([xboxHelper, '--host', host, 'status'], { executable: 'python3' });
  if (state.trim() !== 'ok idle') throw Error('Close Playden and any game before installing.');
  // Require the configured location to exist: never silently recreate an absent external volume.
  if (!(await stat(downloadRoot)).isDirectory()) throw Error('The configured download drive is unavailable.');
  const directory = join(downloadRoot, `${productId}-${choice.contentId}`);
  const helper = fileURLToPath(new URL('install_ps5.py', import.meta.url));
  await command([helper, '--host', host, '--source', directory, '--product', productId, '--title', choice.title, '--preflight'],
    { executable: 'python3' });
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const checkpointPath = `${directory}/.ps5-prepared.json`;
  let prepared = false;
  try {
    const checkpoint = JSON.parse(await readFile(checkpointPath, 'utf8'));
    prepared = checkpoint.schema === 1 && checkpoint.productId === productId && checkpoint.contentId === choice.contentId;
  } catch { /* No completed preparation checkpoint. */ }
  if (!prepared && Number.isSafeInteger(choice.downloadBytes) && choice.downloadBytes > 0) {
    const disk = await statfs(directory);
    const available = disk.bavail * disk.bsize;
    if (available < choice.downloadBytes) throw Error(`Not enough space on the download drive: the package is ${(choice.downloadBytes / 1024 ** 3).toFixed(1)} GiB, but only ${(available / 1024 ** 3).toFixed(1)} GiB is free. Preparation may require additional space. Use a larger download drive or free space before retrying.`);
  }
  job = { stage: 'downloading', title: choice.title, productId, downloadDirectory: directory };
  if (!prepared) await command(['streaming', productId, directory, '--selected-content-id', choice.contentId,
    '--market', market, '--progress-json'], { timeout: 24 * 3600 * 1000, onLine(line) {
      // Only parse structured progress. No backend text or authentication data reaches the page.
      try {
        const progress = JSON.parse(line.startsWith('NEXUS_PROGRESS ') ? line.slice('NEXUS_PROGRESS '.length) : line);
        if (progress.type === 'progress' || progress.event === 'progress') {
          if (Number.isSafeInteger(progress.completedBytes)) job.done = progress.completedBytes;
          if (Number.isSafeInteger(progress.totalBytes)) job.total = progress.totalBytes;
        }
      } catch { /* Non-progress output is intentionally discarded. */ }
    } });
  // Recheck account/subscription after a potentially long download, before preparing or publishing.
  if ((await backend.authorize(productId, choice.contentId)).account !== account)
    throw Error('The Microsoft account changed during the download. Start again with the intended account.');
  job.stage = 'preparing';
  if (!prepared) {
    await command(['prepare-ps5', directory, '--market', market], { timeout: 2 * 3600 * 1000, onLine() {} });
    await writeFile(checkpointPath + '.tmp', JSON.stringify({ schema: 1, productId, contentId: choice.contentId }), { mode: 0o600 });
    await rename(checkpointPath + '.tmp', checkpointPath);
  }
  const stopped = await command([xboxHelper, '--host', host, 'status'], { executable: 'python3' });
  if (stopped.trim() !== 'ok idle') throw Error('Close Playden and any game, then retry. The download has been kept.');
  if (await backend.select() !== account) throw Error('The Microsoft account changed during preparation.');
  await command([xboxHelper, '--host', host, 'signin', '--no-login'], { executable: 'python3', timeout: 120000, onLine() {} });
  job.stage = 'uploading';
  await command([helper, '--host', host, '--source', directory, '--product', productId, '--title', choice.title, '--resume'],
    { executable: 'python3', timeout: 24 * 3600 * 1000, onLine(line) {
      try {
        const progress = JSON.parse(line);
        if (['uploading', 'installed', 'failed'].includes(progress.stage)) job = { ...job, ...progress };
      } catch { /* No unstructured output. */ }
    } });
}

const server = createServer(async (req, res) => {
  try {
    if (req.headers.host !== `127.0.0.1:${port}`) return send(res, 403, { error: 'Invalid host.' });
    const url = new URL(req.url, origin);
    if (req.method === 'GET' && url.pathname === '/') {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store',
        'Referrer-Policy': 'no-referrer', 'X-Frame-Options': 'DENY',
        'Content-Security-Policy': "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src https://store-images.s-microsoft.com https://store-images.microsoft.com; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'" });
      return res.end(html);
    }
    if (req.headers['x-companion-token'] !== token || (req.headers.origin && req.headers.origin !== origin))
      return send(res, 403, { error: 'Open the companion from its local address.' });
    if (req.method === 'GET') {
      if (url.pathname === '/api/job') return send(res, 200, { ...job, busy });
      if (busy) return send(res, 409, { error: 'Wait for the current task to finish.' });
      if (url.pathname === '/api/account') return send(res, 200, await backend.status(url.searchParams.has('refresh')));
      if (url.pathname === '/api/library') return send(res, 200, await backend.page(url.searchParams.get('cursor') ?? undefined, url.searchParams.has('refresh')));
      if (url.pathname === '/api/plan') return send(res, 200, await backend.plan(url.searchParams.get('id') ?? ''));
    }
    if (req.method === 'POST') {
      const input = await body(req);
      if (url.pathname === '/api/login') {
        launch(async () => {
          const existing = await backend.status(true);
          if (existing.signedIn && existing.pcGamePass) {
            job = { stage: 'signed-in', message: 'Your saved Microsoft sign-in is valid. PC Game Pass verified.' };
            return;
          }
          job.stage = 'signing-in';
          job.message = 'Complete sign-in in the separate Xodus login window on this Mac.';
          await command(['login'], { timeout: 10 * 60000, onLine() {} });
          backend.library.reset();
          const status = await backend.status(true);
          if (!status.signedIn) throw Error('Sign-in did not finish.');
          job = { stage: 'signed-in', message: 'Signed in. Refresh your library.' };
        });
        return send(res, 202, { ok: true });
      }
      if (url.pathname === '/api/install') {
        if (!/^[A-Z0-9]{12}$/.test(input.productId ?? '') || typeof input.contentId !== 'string') throw Error('Choose a package first.');
        launch(() => install(input.productId, input.contentId));
        return send(res, 202, { ok: true });
      }
    }
    send(res, 404, { error: 'Not found.' });
  } catch (error) { send(res, 400, { error: error.message }); }
});
server.listen(port, '127.0.0.1', () => console.log(`Xbox companion: ${origin}`));
