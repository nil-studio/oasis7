import {enterKeyEvents} from './viewer-owned-cdp-input.mjs';
import {readOwnedStartupProfile} from './viewer-owned-startup-context.mjs';
import {boundedStartupOperation} from './viewer-owned-startup-lifecycle.mjs';

export async function activateOwnedRetentionOverview({evaluate,run,enter}) {
 const disclosure='.pixel-world-exploration-disclosure';
 const open=await evaluate(`document.querySelector(${JSON.stringify(disclosure)})?.open`);
 if(typeof open!=='boolean')throw Error('retention overview disclosure missing');
 if(!open)await run(['click',disclosure+' > summary']);
 await evaluate(`(()=>{const b=document.querySelector('[data-query-overview]');if(!b||b.disabled||!b.getClientRects().length)throw Error('retention overview unavailable');b.focus({preventScroll:true});if(document.activeElement!==b)throw Error('retention overview focus failed');return true})()`);
 await enter();
 await run(['click',disclosure+' > summary']);
}

// One factory-owned context. No agent-browser process or second WASM instance.
export function createRetentionOwnedInput(context,url) {
 const profile=readOwnedStartupProfile(context),page=context.pages()[0];
 const guard=()=>{if(readOwnedStartupProfile(context)!==profile||context.pages().length!==1||context.pages()[0]!==page||page.url()!==url)throw Error('retention input page changed');};
 const evaluate=code=>{guard();return boundedStartupOperation(page.evaluate(code),'retention evaluate',45000);};
 const run=async args=>{
  guard();let operation;
  if(args[0]==='wait'&&args.length===2&&/^\d+$/.test(args[1])&&Number(args[1])<=10000)operation=new Promise(yes=>setTimeout(yes,Number(args[1])));
  else if(args[0]==='click'&&args.length===2)operation=page.locator(args[1]).click({timeout:10000});
  else if(args[0]==='fill'&&args.length===3)operation=page.locator(args[1]).fill(args[2],{timeout:10000});
  else if(args[0]==='mouse'&&args[1]==='move'&&args.length===4&&args.slice(2).every(x=>Number.isFinite(Number(x))))operation=page.mouse.move(Number(args[2]),Number(args[3]));
  else if(args[0]==='mouse'&&args[1]==='down'&&args.length===2)operation=page.mouse.down();
  else if(args[0]==='mouse'&&args[1]==='up'&&args.length===2)operation=page.mouse.up();
  else if(args[0]==='mouse'&&args[1]==='wheel'&&args.length===3&&Number.isFinite(Number(args[2])))operation=page.mouse.wheel(0,Number(args[2]));
  else throw Error('retention input outside finite allowlist');
  const value=await boundedStartupOperation(operation,'retention native command',45000);guard();return value;
 };
 return {evaluate,run,async createNativeInput(){guard();const session=await boundedStartupOperation(context.newCDPSession(page),'retention native attach');return {async enter(){for(const params of enterKeyEvents()){guard();await boundedStartupOperation(session.send('Input.dispatchKeyEvent',params),'retention Enter');}},async close(){await boundedStartupOperation(session.detach(),'retention native detach');}};}};
}
