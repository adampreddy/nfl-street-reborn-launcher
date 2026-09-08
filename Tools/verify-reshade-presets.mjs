// Check enabled preset techniques against the actual runtime shader bundle.
// This catches renamed/missing shaders; it does not replace a GPU compilation test.
import {readFileSync,readdirSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const runtime=path.resolve(process.argv[2]||path.join(root,'target/installer-payload/runtime'));
const shaderRoot=path.join(runtime,'reshade-shaders/Shaders');
const shaders=new Map();
function visit(dir){for(const entry of readdirSync(dir,{withFileTypes:true})){
 const file=path.join(dir,entry.name);
 if(entry.isDirectory())visit(file);
 else if(entry.name.endsWith('.fx')){
  const sources=shaders.get(entry.name)||[];sources.push(readFileSync(file,'utf8'));shaders.set(entry.name,sources);
 }
}}
visit(shaderRoot);
const report=[];
for(const preset of ['reshade-light.ini','reshade-full.ini']){
 const text=readFileSync(path.join(root,'crates/reborn-core/assets',preset),'utf8');
 const enabled=text.match(/^Techniques=(.*)$/m)?.[1].trim().split(',').filter(Boolean)||[];
 assert.ok(enabled.length,`${preset}: no enabled effects`);
 for(const entry of enabled){
  const [technique,file]=entry.split('@');
  assert.ok(shaders.has(file),`${preset}: missing ${file}`);
  const names=shaders.get(file).flatMap(source=>[...source.matchAll(/\btechnique(?:10|11)?\s+(\w+)/g)].map(m=>m[1]));
  assert.ok(names.includes(technique),`${preset}: ${file} does not declare ${technique}`);
 }
 // Reborn's own uniform names must also match the version being shipped.
 for(const section of text.matchAll(/^\[(NSR_[^\]]+\.fx)\]\r?\n([\s\S]*?)(?=^\[|$(?![\s\S]))/gm)){
  const source=(shaders.get(section[1])||[]).join('\n');
  const uniforms=new Set([...source.matchAll(/\buniform\s+\w+\s+(\w+)/g)].map(m=>m[1]));
  for(const setting of section[2].matchAll(/^(\w+)=/gm))assert.ok(uniforms.has(setting[1]),`${preset}: unknown ${setting[1]} in ${section[1]}`);
 }
 report.push({preset,enabledEffects:enabled.length});
}
console.log(JSON.stringify({runtime,presets:report,scope:'Bundled shader names and Reborn uniform contracts; GPU compilation still requires a real launch.'}));
