import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { CatalogService, AccountLibrary, parseSubscription, readInstallPlan, installSelection } from './nexus-loader.mjs';

export const binary = fileURLToPath(new URL('../../../xodus-ps5-backend/target/release/xodus-cli', import.meta.url));
export const market = 'GB';

export function command(args, { timeout = 65000, executable = binary, onLine } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, { stdio: ['ignore', 'pipe', 'pipe'], shell: false });
    let stdout = '', buffer = '', size = 0, failure;
    const timer = setTimeout(() => { failure = Error('The account service timed out. Try again.'); child.kill('SIGKILL'); }, timeout);
    child.stdout.on('data', chunk => {
      size += chunk.length;
      if (!onLine && size > 16000000) { failure = Error('Backend response exceeded its limit.'); child.kill('SIGKILL'); return; }
      if (!onLine) stdout += chunk;
      else {
        buffer += chunk;
        let end;
        while ((end = buffer.indexOf('\n')) >= 0) {
          const line = buffer.slice(0, end); buffer = buffer.slice(end + 1);
          onLine(line);
        }
        // Progress protocols are bounded; do not retain unbounded diagnostic lines.
        if (buffer.length > 256000) buffer = '';
      }
    });
    // Only the URL-free transfer diagnostic protocol may reach the UI.
    let diagnostics = '', transferFailure;
    child.stderr.on('data', chunk => {
      diagnostics += chunk;
      let end;
      while ((end = diagnostics.indexOf('\n')) >= 0) {
        const line = diagnostics.slice(0, end).trim(); diagnostics = diagnostics.slice(end + 1);
        const match = line.match(/^NEXUS_TRANSFER_DIAGNOSTIC_V1 ([a-z-]{1,64}) ([0-9]{1,3})$/);
        if (match) transferFailure = `Package processing failed (${match[1]}${Number(match[2]) ? '; HTTP ' + match[2] : ''}). Download files have been kept.`;
        const disk = line.match(/^not enough free disk space on .+: need (\d+) bytes, have (\d+) bytes \(files: \d+\)$/);
        if (disk) transferFailure = `Not enough space on the Mac: this download needs ${(Number(disk[1]) / 1024 ** 3).toFixed(1)} GiB free; ${(Number(disk[2]) / 1024 ** 3).toFixed(1)} GiB is available. Download files have been kept.`;
      }
      if (diagnostics.length > 256000) diagnostics = '';
    });
    child.once('error', () => { clearTimeout(timer); reject(Error('Could not start the Xodus backend.')); });
    child.once('close', code => {
      clearTimeout(timer);
      if (failure) reject(failure);
      else if (code !== 0) reject(Error(transferFailure ?? `Xodus ${args[0]} did not complete. No specific diagnostic was supplied.`));
      else resolve(stdout);
    });
  });
}

export async function jsonCommand(args) {
  const text = await command(args);
  const line = text.split('\n').reverse().find(line => line.trimStart().startsWith('{'));
  return JSON.parse(line ?? 'null');
}

export function pcSubscription(profile, now = Date.now()) {
  const sub = parseSubscription(profile?.subscription, now);
  if (!sub || sub.status !== 'active') return false;
  const age = now - Date.parse(sub.checkedAt);
  return age >= -60000 && age < 10 * 60000 &&
    sub.productIds.some(id => id === 'CFQ7TTC0KGQ8' || id === 'CFQ7TTC0KHS0');
}

export class Backend {
  catalog = new CatalogService();
  library = new AccountLibrary((ids, region) => this.catalog.licensedProducts(ids, region));
  account;
  profile;

  async select() {
    const value = await jsonCommand(['account', '--json']);
    const account = value?.stored === true && typeof value.username === 'string' ? value.username : undefined;
    if (account !== this.account) this.profile = undefined;
    this.account = account;
    this.library.select(account, binary, market);
    return account;
  }

  async status(refresh = false) {
    await this.select();
    if (this.account && (refresh || !this.profile)) {
      try {
        const profile = await jsonCommand(['profile', '--json']);
        if (profile?.account !== this.account) throw Error('Account changed.');
        this.profile = profile;
      } catch { this.profile = undefined; }
    }
    return { signedIn: !!this.account, gamertag: this.profile?.gamertag,
      pcGamePass: pcSubscription(this.profile), subscription: parseSubscription(this.profile?.subscription),
      market, console: process.env.PS5_HOST ?? '192.168.0.174' };
  }

  async page(cursor, refresh = false) {
    await this.select();
    return this.library.get('gamepass', cursor, refresh);
  }

  async plan(productId) {
    if (!/^[A-Z0-9]{12}$/.test(productId)) throw Error('Invalid product ID.');
    await this.select();
    const { accessByProductId } = await this.library.access();
    if (accessByProductId[productId] !== 'gamepass') throw Error('This game is not in your current Game Pass library.');
    return readInstallPlan(binary, productId, market);
  }

  async authorize(productId, contentId) {
    const status = await this.status(true);
    if (!status.signedIn || !status.pcGamePass) throw Error('An active PC Game Pass or Ultimate subscription must be verified before installing.');
    const account = this.account;
    const { accessByProductId } = await this.library.access(true);
    if (accessByProductId[productId] !== 'gamepass') throw Error('Microsoft has not granted this account Game Pass access to this game.');
    const plan = await readInstallPlan(binary, productId, market);
    const selected = installSelection(plan, contentId);
    const choice = plan.choices.find(item => item.contentId === selected);
    if (!choice?.supported || choice.kind !== 'Game') throw Error('Choose a supported base PC game.');
    if (await this.select() !== account) throw Error('Account changed during installation checks.');
    return { account, choice };
  }
}
