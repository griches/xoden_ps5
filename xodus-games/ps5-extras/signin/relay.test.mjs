import test, {after} from 'node:test';
import assert from 'node:assert/strict';
import tls from 'node:tls';
import {mkdtemp, readFile, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {execFileSync} from 'node:child_process';
import {createHash, X509Certificate} from 'node:crypto';
import {browserSignIn, pairingInput} from './relay.mjs';

const directory=await mkdtemp(join(tmpdir(),'gamepass-relay-fixture-'));
execFileSync('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-keyout',join(directory,'key.pem'),
  '-out',join(directory,'cert.pem'),'-days','1','-subj','/CN=gamepass-console.local',
  '-addext','subjectAltName=DNS:gamepass-console.local'],{stdio:'ignore'});
const key=await readFile(join(directory,'key.pem')),cert=await readFile(join(directory,'cert.pem'));
const code=createHash('sha256').update(new X509Certificate(cert).raw).digest('hex').slice(0,16);
after(()=>rm(directory,{recursive:true,force:true}));

async function fixture(action) {
  const sockets=new Set();
  const server=tls.createServer({key,cert},socket=>{
    sockets.add(socket);socket.on('error',()=>{});socket.on('close',()=>sockets.delete(socket));
    let buffer='';socket.on('data',chunk=>{
      buffer+=chunk;let end;
      while((end=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,end);buffer=buffer.slice(end+1);action(socket,JSON.parse(line));}
    });
  });
  server.on('tlsClientError',()=>{});
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  return {port:server.address().port,async close(){for(const socket of sockets)socket.destroy();await new Promise(resolve=>server.close(resolve));}};
}

test('pairing accepts only local IPv4 addresses and complete fingerprints',()=>{
  assert.deepEqual(pairingInput('192.168.0.2','ABCD EFFF 1234 5678'),{host:'192.168.0.2',code:'abcdefff12345678'});
  for(const host of ['example.test','8.8.8.8','192.168.0.2/path','127.0.0.1:1234'])assert.throws(()=>pairingInput(host,code));
  assert.throws(()=>pairingInput('127.0.0.1','1234'));
});
test('wrong TV fingerprint sends no application data and never opens Microsoft',async()=>{
  let frames=0,opened=false;
  const server=await fixture(()=>frames++);
  try {
    await assert.rejects(browserSignIn({host:'127.0.0.1',code:'0000000000000000',port:server.port,
      signal:new AbortController().signal,report(){},createView(){opened=true;}}),/does not match/);
    assert.equal(frames,0);assert.equal(opened,false);
  } finally {await server.close();}
});
test('pinned encrypted exchange waits for view setup and explicit saved confirmation',async()=>{
  const updates=[];let disposed=false,callback;
  const server=await fixture((socket,event)=>{
    if(event.type==='hello')socket.write(JSON.stringify({type:'open',session:1,url:'https://login.live.com/ppsecure/InlineLogin.srf',headers:{}})+'\n');
    else {
      assert.equal(event.type,'ipc');assert.equal(event.payload,'synthetic-auth-callback');
      socket.write('{"type":"close","session":1}\n{"type":"complete","saved":true}\n');
    }
  });
  try {
    await browserSignIn({host:'127.0.0.1',code,port:server.port,signal:new AbortController().signal,
      report:update=>updates.push(update),createView:async send=>{
        await new Promise(resolve=>setTimeout(resolve,30));callback=send;
        return {async open(frame){callback(frame.session,'synthetic-auth-callback');},close(){},async dispose(){disposed=true;}};
      }});
    assert.equal(updates.at(-1).stage,'signed-in');assert.equal(disposed,true);
    assert.ok(!JSON.stringify(updates).includes('synthetic-auth-callback'));
  } finally {await server.close();}
});
test('premature success, oversized frames and cancellation never claim saved sign-in',async()=>{
  for(const mode of ['premature','oversized','cancel']) {
    const updates=[],abort=new AbortController();let disposed=false;
    const server=await fixture((socket)=>{
      if(mode==='premature')socket.write('{"type":"complete","saved":true}\n');
      if(mode==='oversized')socket.write('x'.repeat(128*1024+1));
      if(mode==='cancel')abort.abort();
    });
    try {
      await assert.rejects(browserSignIn({host:'127.0.0.1',code,port:server.port,signal:abort.signal,
        report:update=>updates.push(update),createView:async()=>({async dispose(){disposed=true;}})}));
      assert.equal(disposed,true);assert.ok(!updates.some(update=>update.stage==='signed-in'));
    } finally {await server.close();}
  }
});
