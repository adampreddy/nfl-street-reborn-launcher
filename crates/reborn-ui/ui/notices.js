// One-time notices the launcher owes the player (for example: your 2.1
// mapping swapped B and X in Dolphin and the launcher put them back). The
// backend decides what is pending and what it says; this file only draws
// it and reports "Got it".
import {call,el} from './shell.js';

export async function mountNotices(host){
  let list=[];
  try{list=await call('notices_pending');}catch(_){return;}
  if(!Array.isArray(list)||!list.length)return;
  for(const notice of list){
    if(!notice||typeof notice.id!=='string')continue;
    const box=el('section',{class:'notice migration',role:'status','data-notice':notice.id},
      el('strong',{text:notice.title||''}),
      el('p',{text:notice.text||''}),
      el('button',{type:'button',class:'notice-dismiss',text:'Got it',onclick:async()=>{
        try{await call('notice_dismiss',{id:notice.id});}catch(_){}
        box.remove();
      }}));
    host.prepend(box);
  }
}
