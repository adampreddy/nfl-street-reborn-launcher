import {bridge,call,el,get,markCurrent,reason,wireCoreFold} from './shell.js';
const screen=document.getElementById('screen');let busy=false,mode='join',sessionActive=false,room=null,polling=false,disposed=false,lastSnapshot='';
const form=el('form',{class:'netplay-form'}),status=el('p',{class:'settings-status',role:'status'}),submit=el('button',{type:'submit',class:'primary',text:'Join game'});
const tabs=el('div',{class:'section-tabs','aria-label':'Netplay mode'});
for(const [value,label] of [['join','Join a game'],['host','Host a game']])tabs.append(el('button',{type:'button',text:label,'aria-pressed':String(value===mode),onclick:()=>{if(busy)return;mode=value;tabs.querySelectorAll('button').forEach(b=>b.setAttribute('aria-pressed',String(b.textContent===label)));addressRow.hidden=mode==='host';address.required=mode==='join';updateAvailability();}}));
const nickname=el('input',{id:'netplay-name',type:'text',maxlength:32,required:true,placeholder:'Your player name',autocomplete:'nickname',value:localStorage.getItem('netplay:nickname')||''});
const connection=el('select',{id:'netplay-connection'},el('option',{value:'traversal',text:'Host code'}),el('option',{value:'direct',text:'Direct connection'}));
const address=el('input',{id:'netplay-address',type:'text',required:true,maxlength:253,placeholder:'Enter your friend’s host code',spellcheck:'false'}),port=el('input',{id:'netplay-port',type:'number',min:1,max:65535,value:2626,required:true});
const row=(label,id,input)=>el('div',{class:'netplay-field'},el('label',{for:id,text:label}),input);
const addressRow=row('Host code','netplay-address',address),portRow=row('Port','netplay-port',port);portRow.hidden=true;
connection.addEventListener('change',()=>{const direct=connection.value==='direct';addressRow.querySelector('label').textContent=direct?'Host address':'Host code';address.placeholder=direct?'IP address or hostname':'Enter your friend’s host code';portRow.hidden=!direct;});
form.append(el('div',{class:'netplay-intro'},el('h2',{text:'A private game. Your friends.'}),el('p',{text:'Create a room or join with a host code. Up to four players.'})),tabs,row('Player name','netplay-name',nickname),row('Connection','netplay-connection',connection),addressRow,portRow,el('p',{class:'note',text:'Everyone needs the same Reborn version and game file. Your personal memory cards are disconnected for online play; your offline saves stay intact.'}),submit,status);
const lobby=el('section',{class:'netplay-lobby','aria-label':'Netplay room'});lobby.hidden=true;screen.replaceChildren(form,lobby);
function updateAvailability(){
 form.querySelectorAll('input,select,button').forEach(n=>n.disabled=busy||sessionActive||!bridge);
 submit.textContent=busy?'Preparing…':sessionActive?'Game session in progress':mode==='host'?'Create room':'Join game';
 if(busy)screen.setAttribute('aria-busy','true');else screen.removeAttribute('aria-busy');
}
async function action(value,ports){
 if(busy)return;busy=true;
 lobby.querySelectorAll('button,select').forEach(n=>n.disabled=true);
 try{await call('netplay_action',{action:value,ports:ports||null});if(value==='leave'){status.textContent='Leaving room…';}}
 catch(e){status.textContent=reason(e);lobby.append(status);}
 finally{busy=false;lastSnapshot='';await poll();}
}
function drawRoom(snapshot){
 if(!snapshot||typeof snapshot!=='object')return;const previous=room;room=snapshot;const active=!!snapshot.active;if(active&&snapshot.phase!=='running')document.body.dataset.netplayLobby='true';else delete document.body.dataset.netplayLobby;form.hidden=active;lobby.hidden=!active;
 if(!active){if(snapshot.phase==='closed'&&(previous?.active||previous?.phase!=='closed'))status.textContent='You left the room. Your offline save settings are unchanged.';else if(snapshot.error)status.textContent=snapshot.error;if(status.parentNode!==form)form.append(status);if(snapshot.phase!=='idle')sessionActive=false;updateAvailability();return;}
 if(snapshot.phase==='closed'){lobby.replaceChildren(el('h2',{text:'Closing session…'}));return;}
 const signature=JSON.stringify({...snapshot,players:snapshot.players?.map(({ping,...player})=>player)});if(signature===lastSnapshot||busy)return;lastSnapshot=signature;
 // Preserve keyboard/controller focus across server updates.
 const focus=document.activeElement?.dataset?.roomFocus;
 const running=snapshot.phase==='running',host=!!snapshot.host;
 const heading=el('div',{class:'netplay-room-heading'},el('div',{},el('span',{class:'netplay-kicker',text:running?'IN GAME':host?'YOUR ROOM':'NETPLAY'}),el('h2',{text:running?'Game in progress':snapshot.phase==='connecting'?'Connecting…':host?'Ready when you are.':'You’re in.'})),el('span',{class:'netplay-safe',text:'Personal saves protected'}));
 const info=el('p',{class:'note',text:running?'The game is open in its own window. Your room stays here.':host?(snapshot.connection==='direct'?'Share your address and port, assign players, then start the game.':'Share your host code, assign players, then start the game.'):'The host will start the game when everyone is ready.'});
 const code=el('div',{class:'netplay-code'});
 if(host&&snapshot.code){
  // The code, and an invite a friend can paste anywhere: the code in words
  // plus a link that opens their launcher on this room (the installer
  // registers nflstreetreborn:// for it). Adam: "copy invite would be cool".
  // A web link, because chat apps only make http links clickable; the page
  // hands the visitor on to the launcher (Adam: "There's no actual link?").
  const invite=`Join my NFL Street Reborn game! Room code: ${snapshot.code}\nhttps://nflstreetreborn.com/join.php?c=${encodeURIComponent(snapshot.code)}`;
  const copyTo=(text,label)=>async event=>{try{await navigator.clipboard.writeText(text);event.target.textContent='Copied';}catch{event.target.textContent=label;}};
  code.append(el('span',{text:'Host code'}),el('strong',{text:snapshot.code}),el('button',{type:'button','data-room-focus':'copy',text:'Copy',onclick:copyTo(snapshot.code,'Select code to copy')}),el('button',{type:'button','data-room-focus':'invite',text:'Copy invite',onclick:copyTo(invite,'Could not copy')}));
 }
 else if(host&&snapshot.connection==='traversal')code.append(el('span',{text:'Getting your host code…'}));
 else if(host)code.append(el('span',{text:'Direct connection'}),el('strong',{text:`Port ${snapshot.port||port.value}`}),el('span',{class:'note',text:'Share your IP address with your friends.'}));
 const players=el('div',{class:'netplay-players'});
 for(const player of snapshot.players||[])players.append(el('div',{class:'netplay-player'},el('span',{class:'netplay-avatar','aria-hidden':'true',text:(player.name||'?').slice(0,1).toUpperCase()}),el('div',{},el('strong',{text:player.name}),el('small',{text:player.status==='OK'?'Game ready':player.status||'Checking game…'})),el('span',{class:'note',text:player.ping||''})));
 const slots=el('div',{class:'netplay-slots'});
 for(let i=0;i<4;i++){
  const select=el('select',{'aria-label':`Player ${i+1} assignment`,'data-room-focus':`port-${i}`,disabled:!host||running},el('option',{value:'0',text:'Empty'}));
  for(const player of snapshot.players||[])select.append(el('option',{value:String(player.id),text:player.name}));
  select.value=String(snapshot.ports?.[i]||0);
  select.addEventListener('change',()=>{const ports=Array.from(slots.querySelectorAll('select'),n=>Number(n.value));action('ports',ports);});
  slots.append(el('label',{},el('span',{text:`Player ${i+1}`}),select));
 }
 const buttons=el('div',{class:'netplay-actions'},el('button',{type:'button','data-room-focus':'leave',text:running?'End session':'Leave room',onclick:()=>action('leave')}));
 if(host)buttons.append(el('button',{type:'button',class:'primary','data-room-focus':'start',text:running?'Game in progress':'Start game',disabled:!snapshot.can_start||running,onclick:()=>action('start')}));
 lobby.replaceChildren(heading,info,code,players,slots);if(snapshot.error)lobby.append(el('p',{class:'settings-status',role:'alert',text:snapshot.error}));lobby.append(buttons);
 if(focus)lobby.querySelector(`[data-room-focus="${focus}"]`)?.focus({preventScroll:true});
}
async function poll(){if(polling||disposed||!bridge)return;polling=true;try{drawRoom(await call('netplay_status'));}catch(e){if(room?.active){status.textContent=reason(e);lobby.append(status);}}finally{polling=false;}}
form.addEventListener('submit',async event=>{
 event.preventDefault();if(busy||sessionActive||!form.reportValidity())return;
 busy=true;updateAvailability();status.textContent='Preparing your game and controllers…';
 try{await call('netplay_open',{runtime:get('runtime'),game:get('game'),preset:get('preset')||'balanced',nickname:nickname.value.trim(),mode,connection:connection.value,address:address.value.trim(),port:Number(port.value)});localStorage.setItem('netplay:nickname',nickname.value.trim());sessionActive=true;lastSnapshot='';}
 catch(e){status.textContent=reason(e);}finally{busy=false;updateAvailability();await poll();}
});
document.addEventListener('session-state',event=>{sessionActive=!!event.detail;updateAvailability();});
// An invite link (nflstreetreborn://join/<code>) left its room code behind
// when the launcher started: fill the form the way a friend would have read
// it out, then forget the code so a later visit starts clean.
async function takePendingInvite(){
 try{
  const pending=await call('netplay_pending_join');
  if(!pending||typeof pending.code!=='string'||!pending.code)return;
  mode='join';tabs.querySelectorAll('button').forEach(b=>b.setAttribute('aria-pressed',String(b.textContent==='Join a game')));
  connection.value='traversal';connection.dispatchEvent(new Event('change'));
  address.value=pending.code;
  status.textContent=`Invite received: room ${pending.code}. Enter your player name and press Join game.`;
  await call('netplay_pending_join_clear').catch(()=>{});
  updateAvailability();(nickname.value?submit:nickname).focus({preventScroll:true});
 }catch(_){}
}
if(bridge){call('session_active').then(active=>{sessionActive=!!active;updateAvailability();}).catch(()=>{});poll();takePendingInvite();}else status.textContent='Open this page in the launcher to start Netplay.';
const timer=setInterval(poll,750);window.addEventListener('pagehide',()=>{disposed=true;clearInterval(timer);});
updateAvailability();markCurrent();wireCoreFold();
