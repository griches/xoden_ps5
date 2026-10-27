import type {AuthFrame} from './auth-protocol.js';

export interface AuthView {
  open(frame:Extract<AuthFrame,{type:'open'}>):Promise<void>;
  close(session:number):void;
  context(session:number,context:string):Promise<void>;
  dispose():Promise<void>;
}
export type AuthViewFactory=(
  send:(session:number,payload:string)=>void,
  cancel:(session:number)=>void,
  fail:()=>void,
)=>AuthView;
