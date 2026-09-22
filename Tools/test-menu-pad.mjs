import assert from 'node:assert/strict';
import {MenuPadReader, menuDirection, samplePad, bindingValue} from '../crates/reborn-ui/ui/menu-pad.js';
import {detectBinding} from '../crates/reborn-ui/ui/menu-pad-setup.js';
const pad=(index=0)=>({id:'Test controller',index,connected:true,mapping:'standard',buttons:Array.from({length:18},()=>({pressed:false,value:0})),axes:[0,0,0,0]});
let count=0;
const test=(name,fn)=>{fn();console.log('ok '+name);count++;};
const press=(p,i,value=true)=>p.buttons[i]={pressed:value,value:Number(value)};
test('one press moves once; deliberate hold repeats after delay',()=>{
 const r=new MenuPadReader(),p=pad();r.update([p],0);press(p,13);
 assert.equal(r.update([p],16).move,'down');assert.equal(r.update([p],32).move,null);
 r.update([p],200);assert.equal(r.update([p],345).move,null);assert.equal(r.update([p],346).move,'down');
 press(p,13,false);r.update([p],360);press(p,13);assert.equal(r.update([p],380).move,'down');
});
test('second controller operates menus even when the first is idle',()=>{
 const r=new MenuPadReader(),a=pad(),b=pad(1);r.update([a,b],0);press(b,15);
 assert.equal(r.update([a,b],16).move,'right');
});
test('mirrored controllers generate one move and no delayed handoff move',()=>{
 const r=new MenuPadReader(),a=pad(),b=pad(1);r.update([a,b],0);press(a,15);press(b,15);
 assert.equal(r.update([a,b],16).move,'right');press(a,15,false);
 assert.equal(r.update([a,b],32).move,null);press(b,15,false);r.update([a,b],48);
 press(b,13);assert.equal(r.update([a,b],64).move,'down');
});
test('diagonal noise does not alternate directions or restart repeat',()=>{
 const r=new MenuPadReader(),p=pad();r.update([p],0);p.axes=[.7,.69,0,0];assert.equal(r.update([p],16).move,'right');
 p.axes=[.69,.7,0,0];assert.equal(r.update([p],32).move,null);
 p.axes=[.2,.9,0,0];assert.equal(r.update([p],48).move,'down');
});
test('D-pad overrides stick drift and opposite directions cancel',()=>{
 assert.equal(menuDirection({dpad:[true,false,false,false],x:0,y:1},null),'up');
 assert.equal(menuDirection({dpad:[true,true,false,false],x:0,y:1},null),null);
});
test('reconnect and stalls require a fresh neutral before accepting',()=>{
 const r=new MenuPadReader(),p=pad();press(p,0);assert.equal(r.update([p],0).accept,false);
 press(p,0,false);r.update([p],16);press(p,0);assert.equal(r.update([p],32).accept,true);
 assert.equal(r.update([p],500).accept,false);press(p,0,false);r.update([p],516);press(p,0);
 assert.equal(r.update([p],532).accept,true);r.update([],548);assert.equal(r.update([p],564).accept,false);
});
test('unused raw axes cannot keep a standard controller disarmed',()=>{
 const r=new MenuPadReader(),p=pad();p.axes.push(-1);r.update([p],0);press(p,13);
 assert.equal(r.update([p],16).move,'down');
});
test('unmapped raw pads are measured, never assigned standard indices',()=>{
 const p=pad();p.mapping='';assert.equal(samplePad(p,null),null);
 const rest={axes:[0,0,3],buttons:p.buttons};p.axes=[0,-1,3];
 const up=detectBinding(p,rest,'stick');assert.deepEqual(up,{kind:'axis',index:1,rest:0,value:-1});
 const layout={bindings:{stickUp:up,accept:{kind:'button',index:2}}};
 assert.equal(samplePad(p,layout).y,-1);press(p,2);assert.equal(samplePad(p,layout).buttons[0],true);
});
test('raw hat neutral outside [-1,1] is not a held direction',()=>{
 const p=pad();p.mapping='';p.axes=[0,0,-1];
 const rest={axes:[0,0,3],buttons:p.buttons};const up=detectBinding(p,rest,'dpad');
 assert.deepEqual(up,{kind:'hat',index:2,rest:3,value:-1});assert.equal(bindingValue(p,up),1);
 p.axes[2]=3;assert.equal(bindingValue(p,up),0);
});
console.log(`${count} menu input tests passed`);
