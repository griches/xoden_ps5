import {createServer} from 'node:http';
import {spawn} from 'node:child_process';
import {readFile} from 'node:fs/promises';
import {randomBytes,timingSafeEqual} from 'node:crypto';
import {browserSignIn} from './relay.mjs';
import {createView} from './build/browser.mjs';

const port=47832,origin=`http://127.0.0.1:${port}`,token=randomBytes(32).toString('hex');
let consoleHost='';
try {consoleHost=JSON.parse(await readFile(new URL('../companion/config.json',import.meta.url),'utf8')).ps5Host??'';} catch {}
const html=(await readFile(new URL('index.html',import.meta.url),'utf8')).replace('__BOOTSTRAP__',JSON.stringify({token,consoleHost}).replaceAll('<','\\u003c'));
let job={stage:'idle',message:'Open Settings → Xbox account → Sign in with Microsoft on your PS5, then enter the pairing code below.'},controller;
function send(res,code,value){res.writeHead(code,{'Content-Type':'application/json','Cache-Control':'no-store','X-Content-Type-Options':'nosniff','Referrer-Policy':'no-referrer'});res.end(JSON.stringify(value));}
const server=createServer(async(req,res)=>{
  try {
    if(req.headers.host!==`127.0.0.1:${port}`)return send(res,403,{error:'Use the local sign-in page.'});
    if(req.method==='GET' && req.url==='/'){
      res.writeHead(200,{'Content-Type':'text/html; charset=utf-8','Cache-Control':'no-store','Referrer-Policy':'no-referrer','X-Frame-Options':'DENY',
        'Content-Security-Policy':"default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"});return res.end(html);
    }
    const supplied=req.headers['x-signin-token'];
    if(typeof supplied!=='string'||supplied.length!==token.length||!timingSafeEqual(Buffer.from(supplied),Buffer.from(token))||
      (req.headers.origin && req.headers.origin!==origin))return send(res,403,{error:'Reopen the local sign-in page.'});
    if(req.method==='GET'&&req.url==='/api/status')return send(res,200,{...job,busy:!!controller});
    if(req.method==='POST'&&req.url==='/api/cancel'){controller?.abort();return send(res,202,{ok:true});}
    if(req.method==='POST'&&req.url==='/api/connect'){
      if(controller)return send(res,409,{error:'Finish or cancel the current sign-in first.'});
      let bytes=0,text='';for await(const chunk of req){bytes+=chunk.length;if(bytes>2048)throw Error('Request too large.');text+=chunk;}
      const input=JSON.parse(text),current=new AbortController();controller=current;
      job={stage:'pairing',message:'Connecting to your PS5…'};
      browserSignIn({host:input.host,code:input.code,signal:current.signal,report:value=>{job=value;},createView})
        .catch(error=>{job={stage:current.signal.aborted?'cancelled':'failed',message:current.signal.aborted?'Sign-in cancelled. You can try again.':error.message};})
        .finally(()=>{if(controller===current)controller=undefined;});
      return send(res,202,{ok:true});
    }
    return send(res,404,{error:'Not found.'});
  }catch(error){send(res,400,{error:error instanceof SyntaxError?'Invalid request.':error.message});}
});
server.listen(port,'127.0.0.1',()=>{
  console.log(`XoDen sign-in: ${origin}`);
  if(process.argv.includes('--open') && process.platform==='darwin')spawn('open',[origin],{stdio:'ignore'}).on('error',()=>{});
});
for(const signal of ['SIGINT','SIGTERM'])process.once(signal,()=>{controller?.abort();server.close();});
