import {readLayout} from './menu-pad.js';
export const REPORT_STEPS=[
 ['rest','Release all controls'],['dpad_up','Press D-pad up'],['dpad_down','Press D-pad down'],
 ['dpad_left','Press D-pad left'],['dpad_right','Press D-pad right'],
 ['stick_up','Move the left stick up'],['stick_down','Move the left stick down'],
 ['stick_left','Move the left stick left'],['stick_right','Move the left stick right'],
 ['accept','Press the button you use to select'],['back','Press the button you use to go back']
];
const actions=['up','down','left','right','stickUp','stickDown','stickLeft','stickRight','accept','back','previous','next'];
function menuLayout(pad,storage){
 const layout=readLayout(pad,storage);if(!layout?.bindings)return null;
 const bindings={};
 for(const key of actions){const b=layout.bindings[key];if(!b)continue;
  if(!['button','axis','hat'].includes(b.kind)||!Number.isInteger(b.index)||b.index<0||b.index>=(b.kind==='button'?pad.buttons.length:pad.axes.length))return null;
  bindings[key]={kind:b.kind,index:b.index};
  if(b.kind!=='button'){
   if(!Number.isFinite(b.rest)||!Number.isFinite(b.value)||b.rest===b.value)return null;
   Object.assign(bindings[key],{rest:b.rest,value:b.value});
  }
 }
 return {bindings};
}
export class ControllerRecording {
 constructor(metadata){this.data={...metadata,devices:[],steps:[]};this.keys=new Map();}
 sample(pads,t,focused,storage){
  const values=[];
  for(const pad of [...pads].filter(p=>p?.connected).slice(0,8)){
   const key=`${pad.id}:${pad.mapping}:${pad.axes.length}:${pad.buttons.length}`;
   if(!this.keys.has(key)){
    if(this.data.devices.length>=32)continue;
    this.keys.set(key,this.data.devices.length);
    this.data.devices.push({id:pad.id.slice(0,512),mapping:pad.mapping.slice(0,32),axes:Math.min(64,pad.axes.length),buttons:Math.min(128,pad.buttons.length),menu_layout:menuLayout(pad,storage)});
   }
   values.push({device:this.keys.get(key),index:pad.index,axes:pad.axes.slice(0,64),
    buttons:pad.buttons.slice(0,128).map(b=>[b.value,!!b.pressed])});
  }
  return {t:Math.round(t),focused:!!focused,pads:values};
 }
 step(name,status='skipped'){
  const step={name,status,runtime_status:'not_requested',binding:null,samples:[]};this.data.steps.push(step);return step;
 }
}
