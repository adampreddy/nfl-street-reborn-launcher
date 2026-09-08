import {bridge,call,el,get,markCurrent,reason,wireCoreFold} from './shell.js';
const screen=document.getElementById('screen');let busy=false,mode='join',sessionActive=false,handedOff=false;
const banner=el('section',{class:'online-coming'},el('span',{class:'store-kicker',text:'MATCHMAKING COMING SOON'}),el('h2',{text:'Your next rival is out there.'}),el('p',{text:'For now, set up a private game with friends using Dolphin Netplay.'}));
const form=el('form',{class:'netplay-form'}),status=el('div',{class:'settings-status',role:'status'}),submit=el('button',{type:'submit',class:'primary',text:'Continue in Dolphin'});
const tabs=el('div',{class:'section-tabs','aria-label':'Netplay mode'});
for(const [value,label] of [['join','Join a game'],['host','Host a game']])tabs.append(el('button',{type:'button',text:label,'aria-pressed':String(value===mode),onclick:()=>{if(busy)return;mode=value;tabs.querySelectorAll('button').forEach(b=>b.setAttribute('aria-pressed',String(b.textContent===label)));addressRow.hidden=mode==='host';address.required=mode==='join';}}));
const nickname=el('input',{id:'netplay-name',type:'text',maxlength:32,required:true,placeholder:'Your player name',autocomplete:'nickname'});
const connection=el('select',{id:'netplay-connection'},el('option',{value:'traversal',text:'Host code'}),el('option',{value:'direct',text:'Direct connection'}));
const address=el('input',{id:'netplay-address',type:'text',required:true,maxlength:253,placeholder:'Enter your friend’s host code',spellcheck:'false'}),port=el('input',{id:'netplay-port',type:'number',min:1,max:65535,value:2626,required:true});
const row=(label,id,input,description)=>el('div',{class:'netplay-field'},el('label',{for:id,text:label}),input,description?el('p',{class:'note',text:description}):null);
const addressRow=row('Host code','netplay-address',address),portRow=row('Port','netplay-port',port);portRow.hidden=true;
connection.addEventListener('change',()=>{const direct=connection.value==='direct';addressRow.querySelector('label').textContent=direct?'Host address':'Host code';address.placeholder=direct?'IP address or hostname':'Enter your friend’s host code';portRow.hidden=!direct;});
form.append(el('h2',{text:'Play with friends'}),tabs,row('Player name','netplay-name',nickname),row('Connection','netplay-connection',connection),addressRow,portRow,el('p',{class:'note',text:'Both players need the same Reborn version and game file. The host’s save data and game settings are shared for the session.'}),submit,status);
function updateAvailability(){
 form.querySelectorAll('input,select,button').forEach(n=>n.disabled=busy||sessionActive||!bridge);
 submit.textContent=busy?'Preparing…':sessionActive?'Dolphin is open':'Continue in Dolphin';
 if(busy)screen.setAttribute('aria-busy','true');else screen.removeAttribute('aria-busy');
}
function sessionChanged(active){
 sessionActive=!!active;updateAvailability();
 if(!busy&&!handedOff&&sessionActive)status.textContent='Close Dolphin before starting another session.';
 if(!busy&&handedOff&&!sessionActive){handedOff=false;status.textContent='Dolphin is closed. You can start another session.';}
}
form.addEventListener('submit',async event=>{
 event.preventDefault();if(busy||sessionActive||!form.reportValidity())return;
 busy=true;updateAvailability();status.textContent='Preparing your game and controllers…';
 try{
  await call('netplay_open',{runtime:get('runtime'),game:get('game'),preset:get('preset')||'balanced',nickname:nickname.value.trim(),mode,connection:connection.value,address:address.value.trim(),port:Number(port.value)});
  handedOff=true;sessionActive=true;
  status.replaceChildren(el('h3',{text:'Continue in Dolphin'}),el('ol',{},
   el('li',{text:'Open Tools → Start Netplay.'}),
   el('li',{text:mode==='host'?'Choose the Host tab, select your Reborn game, then click Host.':'Your connection details are filled in. Click Connect.'}),
   el('li',{text:mode==='host'?(connection.value==='direct'?'Share your address and port with your friend. Start when everyone is ready.':'Share the host code with your friend. Start the game when everyone is ready.'):'Wait for the host to start the game.'})));
 }catch(e){status.textContent=reason(e);}finally{busy=false;updateAvailability();}
});
screen.replaceChildren(banner,form);
document.addEventListener('session-state',event=>sessionChanged(event.detail));
if(bridge){call('session_active').then(sessionChanged).catch(()=>{});}else{status.textContent='Open this page in the launcher to start Netplay.';}
updateAvailability();markCurrent();wireCoreFold();
