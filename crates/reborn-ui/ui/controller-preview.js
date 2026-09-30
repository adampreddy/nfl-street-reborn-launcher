// Artwork by ControllerOverlays contributors; see art/controllers/NOTICE.txt.
(() => {
 const cache = new Map();
 function model(id) {
  if (/dualsense|0ce6|0df2|ps5/i.test(id)) return ['dualsense','DualSense'];
  if (/xbox|xinput|045e/i.test(id)) return ['xbox','Xbox One layout'];
  if (/gamecube|game cube|wup-028/i.test(id)) return ['gamecube','GameCube'];
  if (/dualshock|playstation|054c|wireless controller|ps4/i.test(id) && !/dualsense|0ce6|ps5/i.test(id)) return ['playstation','DualShock 4'];
  return [null,'Controller model unavailable'];
 }
 window.createControllerPreview = id => {
  const [kind,title] = model(id);
  const host = document.createElement('figure');host.className='controller-preview sourced-preview';host.dataset.family=kind||'generic';host.setAttribute('role','img');host.setAttribute('aria-label',title);
  const caption=document.createElement('figcaption');caption.textContent=title;host.append(caption);
  let svg=null;
  if(['dualsense','xbox','playstation'].includes(kind)) {
   const folder=({xbox:'xbox-one',playstation:'ds4'})[kind]||kind;
   const width=({xbox:750,playstation:806,dualsense:1200})[kind];
   let style=document.querySelector(`link[data-controller-art="${folder}"]`);if(!style){style=document.createElement('link');style.rel='stylesheet';style.href=`art/controllers/${folder}/preview.css`;style.dataset.controllerArt=folder;document.head.append(style);}
   fetch(`art/controllers/${folder}/template.html`).then(r=>r.text()).then(text=>{
    const template=document.createElement('template');template.innerHTML=text;template.content.querySelectorAll('script,link').forEach(n=>n.remove());
    const pad=document.createElement('div');pad.className=`vendor-${folder}`;pad.dataset.color='white';pad.append(template.content);host.prepend(pad);host.classList.add('layered-preview');host.style.setProperty('--controller-ratio',String(width/({xbox:630,playstation:598,dualsense:700})[kind]));
    const resize=()=>pad.style.transform=`scale(${host.clientWidth/width})`;
    const observer=new ResizeObserver(()=>{if(!host.isConnected){observer.disconnect();return;}resize();});observer.observe(host);resize();
    host.update=gamepad=>pad.querySelectorAll('[data-button]').forEach(n=>{const b=gamepad?.buttons?.[Number(n.dataset.button)];n.dataset.pressed=String(!!b?.pressed);n.dataset.value=String(b?.value||0);if(n.classList.contains('trigger'))n.style.clipPath=`inset(${100-(b?.value||0)*100}% 0 0 0)`;if(n.dataset.axisX)n.style.translate=`${(gamepad?.axes?.[Number(n.dataset.axisX)]||0)*20}px ${(gamepad?.axes?.[Number(n.dataset.axisY)]||0)*20}px`;});
   }).catch(()=>caption.textContent='Controller image unavailable');
  } else if(kind) {
   if(!cache.has(kind)) cache.set(kind,fetch(`art/controllers/${kind}.svg`).then(r=>{if(!r.ok)throw Error('art');return r.text();}));
   cache.get(kind).then(text=>{
    const parsed=new DOMParser().parseFromString(text,'image/svg+xml');
    svg=document.importNode(parsed.documentElement,true);svg.setAttribute('role','img');svg.setAttribute('aria-label',title);svg.removeAttribute('width');svg.removeAttribute('height');host.prepend(svg);
   }).catch(()=>caption.textContent='Controller image unavailable');
  }
  host.update=pad=>{
   if(!svg)return;
   // A pressed control gets a solid red outline, no glow and no blur (owner rule). The art has no strokes of
   // its own, so a stroke on the control's group reaches every shape in it; 28 user units is about 2px on screen.
   const names=['Button_A','Button_B','Button_X','Button_Y','Button_L','Button_R','Analog_L','Analog_R','Button_Select','Button_Start','Analog_Stick','Analog_C_Stick','Dpad_Up','Dpad_Down','Dpad_Left','Dpad_Right'];
   names.forEach((name,i)=>{const node=svg.querySelector(`[id="${name}"]`);if(node){const on=!!pad?.buttons?.[i]?.pressed;node.style.stroke=on?'var(--red,#E0272E)':'';node.style.strokeWidth=on?'28px':'';}});
   ['Analog_Stick','Analog_C_Stick'].forEach((name,i)=>{const node=svg.querySelector(`[id="${name}"]`);if(node)node.style.transform=`translate(${(pad?.axes?.[i*2]||0)*22}px,${(pad?.axes?.[i*2+1]||0)*22}px)`;});
  };
  return host;
 };
 function mount(){
  document.querySelectorAll('.page-setup .choice:has(input[name="port"]),.page-setup .choice:has(input[name="profile"])').forEach(label=>{
   if(label.dataset.previewMounted)return;label.dataset.previewMounted='true';
   const input=label.querySelector('input');if(input.value==='none')return;
   const id=input.name==='profile'?({xbox:'Xbox One',playstation:'DualShock 4',gamecube:'GameCube'})[input.value]:`${label.querySelector('.what')?.textContent} ${label.querySelector('.why')?.textContent}`;
   label.append(window.createControllerPreview(id||''));
  });
 }
 new MutationObserver(mount).observe(document.getElementById('screen'),{childList:true,subtree:true});mount();
})();
