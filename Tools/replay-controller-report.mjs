// Offline replay of a report's browser input through the production menu reader.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {MenuPadReader,menuDirection,samplePad} from '../crates/reborn-ui/ui/menu-pad.js';

export function replayReport(report){
 if(report.schema!=='nsr.controller-report.v1'||!Array.isArray(report.recording?.devices)||!Array.isArray(report.recording?.steps))throw Error('Unsupported controller report');
 const descriptors=report.recording.devices;
 return report.recording.steps.map(step=>{
  const reader=new MenuPadReader(),events=[],directions=new Set();let normalized=false;
  for(const frame of step.samples){
   const pads=frame.pads.map(p=>{const d=descriptors[p.device];if(!d)throw Error('Missing controller descriptor');
    return {index:p.index,id:d.id,connected:true,mapping:d.mapping,axes:p.axes,
     buttons:p.buttons.map(([value,pressed])=>({value,pressed})),layout:d.menu_layout};});
   if(!frame.focused){reader.reset();continue;}
   for(const pad of pads){const sample=samplePad(pad,pad.layout);if(sample){normalized=true;const direction=menuDirection(sample,null);if(direction)directions.add(direction);}}
   const result=reader.update(pads,frame.t,p=>p.layout);
   for(const action of ['move','accept','back','previous','next'])if(result[action])events.push({t:frame.t,action,value:result[action],index:result.pad?.index});
  }
  const expected=/^(dpad|stick)_(up|down|left|right)$/.exec(step.name)?.[2]||null;
  return {step:step.name,status:step.status,expected,normalized,observed_directions:[...directions],
   direction_mismatch:!!expected&&[...directions].some(d=>d!==expected),events,runtime_status:step.runtime_status,runtime_binding:step.binding};
 });
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 if(!process.argv[2]){console.error('Usage: node Tools/replay-controller-report.mjs <report.json>');process.exitCode=1;}
 else{try{console.log(JSON.stringify(replayReport(JSON.parse(readFileSync(process.argv[2],'utf8'))),null,2));}catch(e){console.error(e.message);process.exitCode=1;}}
}
