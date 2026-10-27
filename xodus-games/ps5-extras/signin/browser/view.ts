import { access, constants } from 'node:fs/promises';
import { createBrowserAuthViewFactory } from './auth-browser.js';
export { authFrame, AUTH_FRAME_LIMIT, AUTH_PREFIX } from './auth-protocol.js';

export async function createView(send: (session:number,payload:string)=>void,cancel:(session:number)=>void,fail:()=>void) {
  const candidates = process.platform === 'darwin'
    ? ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', process.env.HOME+'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome']
    : process.platform === 'win32'
      ? [process.env.PROGRAMFILES+'\\Google\\Chrome\\Application\\chrome.exe',process.env.LOCALAPPDATA+'\\Google\\Chrome\\Application\\chrome.exe']
      : ['/usr/bin/google-chrome-stable','/usr/bin/google-chrome','/opt/google/chrome/google-chrome'];
  for (const binary of candidates) {
    try { await access(binary,constants.X_OK); }
    catch { continue; }
    return createBrowserAuthViewFactory(binary,process.env,{diagnostic:code=>process.stderr.write('SIGNIN_VIEW '+code+'\n')})(send,cancel,fail);
  }
  throw Error('Install Google Chrome on this computer, then try again. Microsoft sign-in uses a separate temporary Chrome window.');
}
