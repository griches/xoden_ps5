// Same loopback bridge as Xodus Nexus (src/main/xodus-service.ts): 64-char key, then a pipe to xodus.sock
import { createConnection, createServer } from 'node:net';
import { timingSafeEqual } from 'node:crypto';
const [port, key, socketPath] = [Number(process.argv[2]), process.argv[3], process.argv[4]];
const expected = Buffer.from(key, 'ascii');
let n = 0;
createServer((client) => {
  const id = ++n;
  let received = Buffer.alloc(0);
  const authenticate = (chunk) => {
    received = Buffer.concat([received, chunk]);
    if (received.length < expected.length) return;
    if (!timingSafeEqual(received.subarray(0, expected.length), expected)) { console.log(`#${id} bad key`); client.destroy(); return; }
    const remaining = received.subarray(expected.length);
    client.removeListener('data', authenticate);
    client.pause();
    const upstream = createConnection(socketPath);
    upstream.once('error', (e) => { console.log(`#${id} upstream error ${e.message}`); client.destroy(); });
    client.once('error', () => upstream.destroy());
    upstream.once('connect', () => {
      console.log(`#${id} connected`);
      if (remaining.length) upstream.write(remaining);
      client.pipe(upstream); upstream.pipe(client); client.resume();
    });
    client.once('close', () => console.log(`#${id} closed`));
  };
  client.on('data', authenticate);
  client.on('error', () => client.destroy());
}).listen(port, '127.0.0.1', () => console.log(`bridge on 127.0.0.1:${port}`));
