import tls from 'node:tls';
import net from 'node:net';
import { createHash, timingSafeEqual, X509Certificate } from 'node:crypto';
import {authFrame, AUTH_PREFIX} from './build/browser.mjs';

const NAME='gamepass-console.local';
const LIMIT=128*1024;
export function pairingInput(host,code) {
  if (net.isIP(host)!==4 || !(/^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|127\.)/.test(host))) throw Error('Enter the local IPv4 address shown in the console settings.');
  code=String(code).replace(/[\s-]/g,'').toLowerCase();
  if (!/^[a-f0-9]{16}$/.test(code)) throw Error('Enter all 16 characters of the pairing code shown on the TV.');
  return { host,code };
}

async function certificate(host,code,signal,port) {
  return new Promise((resolve,reject)=>{
    // Discovery only: no login frame or credential is sent on this socket.
    // The next connection uses this certificate as its verified trust anchor.
    const socket=tls.connect({host,port,servername:NAME,rejectUnauthorized:false,signal});
    socket.setTimeout(5000,()=>socket.destroy(Error('The PS5 did not respond. Open Microsoft sign-in in the app and try again.')));
    socket.once('error',()=>reject(Error(signal?.aborted?'Sign-in cancelled.':'Could not reach the PS5. Open Microsoft sign-in in the app and check the address.')));
    socket.once('secureConnect',()=>{
      try {
        const der=socket.getPeerCertificate().raw;
        const actual=createHash('sha256').update(der).digest('hex').slice(0,16);
        if (!timingSafeEqual(Buffer.from(actual),Buffer.from(code))) throw Error('The pairing code does not match this PS5. Check the code on the TV and retry.');
        resolve(new X509Certificate(der).toString());
      } catch(error) { reject(error); }
      finally { socket.destroy(); }
    });
  });
}

export async function browserSignIn({host,code,signal,report,createView,port=47834}) {
  ({host,code}=pairingInput(host,code));
  report({stage:'pairing',message:'Checking the pairing code with your PS5…'});
  const ca=await certificate(host,code,signal,port);
  if(signal?.aborted) throw Error('Sign-in cancelled.');
  let socket;
  let view,active,latest=0,queue=Promise.resolve(),buffer=Buffer.alloc(0),finished=false;
  let resolveResult,rejectResult;
  const result=new Promise((resolve,reject)=>{resolveResult=resolve;rejectResult=reject;});
  const fail=error=>{if(!finished){finished=true;rejectResult(error instanceof Error?error:Error('The sign-in connection closed. Try again.'));}};
  const write=event=>{if(!finished && socket && !socket.destroyed) socket.write(JSON.stringify(event)+'\n');};
  const timer=setTimeout(()=>fail(Error('Sign-in expired after ten minutes. Start Microsoft sign-in again.')),600000);
  result.catch(()=>{});
  try {
    view=await createView((session,payload)=>{
      if(session===active && typeof payload==='string' && Buffer.byteLength(payload)<=65536) write({type:'ipc',session,payload});
    },session=>{if(session===active){write({type:'closed',session});fail(Error('Microsoft sign-in was closed. You can try again.'));}},()=>fail(Error('The Microsoft sign-in window was interrupted. Your PS5 account has not been changed. Try again.')));
    if(signal?.aborted)throw Error('Sign-in cancelled.');
    socket=tls.connect({host,port,servername:NAME,ca,rejectUnauthorized:true,signal});
    socket.on('error',()=>fail(Error(signal?.aborted?'Sign-in cancelled.':'The secure PS5 connection was interrupted. Start Microsoft sign-in again.')));
    socket.on('close',()=>{queue.then(()=>{if(!finished)fail(Error('The PS5 ended sign-in before confirming it was saved. Check the console message.'));});});
    socket.on('secureConnect',()=>{write({type:'hello',version:1});report({stage:'signing-in',message:'Complete Microsoft sign-in in the window that opens. Use the Microsoft account you want on your PS5.'});});
    socket.on('data',chunk=>{
      buffer=Buffer.concat([buffer,chunk]);
      let end;
      while((end=buffer.indexOf(10))>=0){
        const line=buffer.subarray(0,end);buffer=buffer.subarray(end+1);
        if(line.length>LIMIT){fail(Error('The PS5 sent an oversized sign-in response.'));return;}
        queue=queue.then(async()=>{
          if(finished)return;
          let value;try{value=JSON.parse(line.toString('utf8'));}catch{throw Error('Invalid PS5 sign-in response.');}
          if(value.type==='failed'){
            if(active!==undefined || latest===0 || value.approvalSaved!==true)throw Error('The PS5 did not confirm Microsoft approval.');
            if(value.code==='browser-account-mismatch')throw Error('That Microsoft account does not match your PS5 account. Sign out on the PS5 before changing accounts.');
            const rejection=Array.isArray(value.rejections)?value.rejections.at(-1):undefined;
            const http=Number.isInteger(rejection?.http)&&rejection.http>=400&&rejection.http<=599?rejection.http:undefined;
            const xerr=Number.isSafeInteger(rejection?.xerr)&&rejection.xerr>=0?rejection.xerr:undefined;
            const kind=['connect','timeout','request','response'].includes(rejection?.kind)?rejection.kind:undefined;
            const detail=http?` (HTTP ${http}${xerr!==undefined?`, Xbox code ${xerr}`:''})`:kind?` (${kind})`:'';
            throw Error(`Microsoft approval is saved privately on your PS5, but Xbox verification did not finish${detail}. Choose Sign in with Microsoft on the PS5 to retry verification without logging in again.`);
          }
          if(value.type==='complete'){
            if(active!==undefined || latest===0 || value.saved!==true)throw Error('The PS5 did not confirm a completed sign-in.');
            report({stage:'signed-in',message:'Sign-in saved securely on your PS5. You can close this page and return to your console.'});
            finished=true;resolveResult();return;
          }
          const frame=authFrame(AUTH_PREFIX+line.toString('utf8'));
          if(frame?.type==='open'){
            if(frame.session<=latest || frame.session>8)throw Error('Invalid PS5 sign-in order.');
            active=latest=frame.session;await view.open(frame);
          }else if(frame?.session===active){
            if(frame.type==='close'){view.close(active);active=undefined;report({stage:'verifying',message:'The PS5 is checking the account and saving your sign-in…'});}
            else if(frame.type==='context')await view.context(active,frame.context);
            else throw Error('Invalid PS5 sign-in response.');
          }else throw Error('Unexpected PS5 sign-in response.');
        }).catch(fail);
      }
      if(buffer.length>LIMIT)fail(Error('The PS5 sent an oversized sign-in response.'));
    });
    await result;
  } finally {
    clearTimeout(timer);socket?.destroy();await view?.dispose().catch(()=>{});
  }
}
