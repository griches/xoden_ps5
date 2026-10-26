// Read-only view of the running companion; does not restart its download.
import { createServer } from 'node:http';
const upstream = 'http://127.0.0.1:47831';
const page = `<!doctype html><meta charset="utf-8"><title>PS5 installation progress</title>
<style>:root{color-scheme:dark;font:18px system-ui;background:#101612;color:#eef5ef}main{max-width:700px;margin:12vh auto;padding:28px}progress{width:100%;accent-color:#9bea9f}p{color:#afc1b3}a{color:#9bea9f}</style>
<main><p>Game Pass · PS5 installation</p><h1 id="title">Checking installation…</h1><h2 id="stage"></h2><progress id="bar"></progress><p id="amount"></p><p id="message"></p><a href="${upstream}">Back to library</a></main>
<script>const $=id=>document.getElementById(id);function size(n){return(n/1024**3).toFixed(2)+' GB'}async function poll(){try{const r=await fetch('/status');if(!r.ok)throw Error('Cannot reach the companion.');const j=await r.json();$('title').textContent=j.title||'Installation';$('stage').textContent=j.stage.replaceAll('-',' ');$('message').textContent=j.message||'';if(Number.isFinite(j.total)&&j.total>0){$('bar').max=j.total;$('bar').value=j.done||0;$('amount').textContent=size(j.done||0)+' / '+size(j.total)+' · '+Math.floor((j.done||0)/j.total*100)+'%'}else{$('bar').removeAttribute('value');$('amount').textContent='Waiting for byte totals from the installer.'}if(!j.busy){$('bar').hidden=true;$('amount').textContent=j.stage==='installed'?'Transfer finished. Playden preparation and Xbox runtime setup are still required.':$('amount').textContent}}catch(e){$('message').textContent=e.message}setTimeout(poll,1500)}poll();</script>`;
createServer(async(req,res)=>{
  res.setHeader('Cache-Control','no-store');
  if(req.headers.host!=='127.0.0.1:47833'){res.writeHead(403);return res.end()}
  if(req.url==='/'){res.setHeader('Content-Type','text/html; charset=utf-8');return res.end(page)}
  if(req.url!=='/status'){res.writeHead(404);return res.end()}
  try{
    const html=await(await fetch(upstream,{signal:AbortSignal.timeout(5000)})).text();
    const token=html.match(/const token='([^']+)'/)?.[1];
    if(!token)throw Error('Missing session');
    const response=await fetch(upstream+'/api/job',{headers:{'X-Companion-Token':token},signal:AbortSignal.timeout(5000)});
    if(!response.ok)throw Error('Status unavailable');
    res.setHeader('Content-Type','application/json');res.end(JSON.stringify(await response.json()));
  }catch{res.writeHead(503);res.end('{}')}
}).listen(47833,'127.0.0.1',()=>console.log('Installation progress: http://127.0.0.1:47833'));
