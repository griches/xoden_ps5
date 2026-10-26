// Local, video-only viewer of the existing PS5 DevLoop Remote Play session.
import { createServer } from 'node:http';
import { createConnection } from 'node:net';
import { randomBytes } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const port = 47832;
const token = randomBytes(32).toString('hex');
const socket = fileURLToPath(new URL('../../../PS5_DevLoop/.rp/serve.sock', import.meta.url));
function frame() {
  return new Promise((resolve, reject) => {
    const client = createConnection(socket);
    const chunks = []; let length = 0;
    client.setTimeout(5000, () => client.destroy(Error('Remote Play did not answer.')));
    client.on('connect', () => client.write('look\n'));
    client.on('data', chunk => {
      length += chunk.length;
      if (length > 10000000) client.destroy(Error('Frame too large.'));
      else chunks.push(chunk);
    });
    client.on('error', () => reject(Error('Remote Play is not connected.')));
    client.on('end', () => {
      const data = Buffer.concat(chunks), newline = data.indexOf(10);
      const header = data.subarray(0, newline).toString().split(' ');
      const png = data.subarray(newline + 1);
      if (header[0] !== 'ok' || Number(header[1]) !== png.length || png.readUInt32BE(0) !== 0x89504e47)
        return reject(Error('Waiting for a Remote Play picture.'));
      resolve({ png, age: header[3] });
    });
  });
}
const page = `<!doctype html><html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>PS5 · Live Remote Play</title>
<style>:root{color-scheme:dark;font:15px system-ui;background:#080b10;color:#e4edf5}body{margin:0;display:flex;min-height:100vh;flex-direction:column}header{padding:16px 24px;display:flex;align-items:center;gap:16px}strong{font-size:18px}span{color:#98acbc}button{margin-left:auto;padding:9px 15px;background:#202d3b;color:inherit;border:1px solid #405165;border-radius:7px;cursor:pointer}main{flex:1;display:grid;place-items:center}img{width:100%;max-height:calc(100vh - 75px);object-fit:contain}#state{font-size:13px}</style>
<header><strong>PS5 · Live view</strong><span id="state">Connecting…</span><button id="full">Full screen</button></header><main><img id="screen" alt="Live PS5 screen"></main>
<script>const token='${token}',screen=document.getElementById('screen'),state=document.getElementById('state');let previous;
document.getElementById('full').onclick=()=>document.documentElement.requestFullscreen();
async function refresh(){try{const r=await fetch('/frame',{headers:{'X-View-Token':token},cache:'no-store'});if(!r.ok)throw Error('Waiting for Remote Play…');const next=URL.createObjectURL(await r.blob());screen.src=next;if(previous)URL.revokeObjectURL(previous);previous=next;const age=Number(r.headers.get('X-Frame-Age'));state.textContent=age>1000?'Stream paused · last picture':'Live · video only';}catch(e){state.textContent=e.message}setTimeout(refresh,document.hidden?1000:100)}refresh();</script></html>`;

createServer(async (req, res) => {
  if (req.headers.host !== `127.0.0.1:${port}`) { res.writeHead(403); return res.end(); }
  res.setHeader('Cache-Control', 'no-store');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Frame-Options', 'DENY');
  if (req.url === '/' && req.method === 'GET') {
    res.setHeader('Content-Type', 'text/html; charset=utf-8');
    res.setHeader('Content-Security-Policy', "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src blob:; connect-src 'self'; frame-ancestors 'none'");
    return res.end(page);
  }
  if (req.url !== '/frame' || req.method !== 'GET' || req.headers['x-view-token'] !== token) { res.writeHead(403); return res.end(); }
  try {
    const { png, age } = await frame();
    res.setHeader('Content-Type', 'image/png'); res.setHeader('X-Frame-Age', age); res.end(png);
  } catch { res.writeHead(503); res.end('Remote Play unavailable'); }
}).listen(port, '127.0.0.1', () => console.log(`PS5 live view: http://127.0.0.1:${port}`));
