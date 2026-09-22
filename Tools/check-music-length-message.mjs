// The Music page's "this song is too long" sentence, run out of music.js
// itself so it cannot drift from what ships. The cap is the game's own stream
// buffer and comes from the core (music_status.limit_seconds), so the page
// must never write a length of its own down: before the fix it read
// `mmss(limit_seconds || 0)`, and mmss(0) is the truthy string "0:00", so a
// status that had not been read yet promised the player a cap of no seconds
// at all. Needs no browser, so it runs where playwright is not installed.
//
//     node Tools/check-music-length-message.mjs
import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
const src=readFileSync(new URL('../crates/reborn-ui/ui/music.js',import.meta.url),'utf8');

// Lift a function out of the real source by walking its braces.
function grab(name){const at=src.indexOf(`function ${name}(`);assert.ok(at>=0,`${name} is gone from music.js`);
 let depth=0;for(let j=src.indexOf('{',at);j<src.length;j++){if(src[j]==='{')depth++;else if(src[j]==='}'&&--depth===0)return src.slice(at,j+1);}
 throw Error(`unbalanced ${name}`);}
const build=new Function('statusValue',`let status=statusValue;${grab('mmss')}${grab('tooLongProblem')}return tooLongProblem;`);

// A cap and a length: both numbers, in the same words the core uses.
let m=build({limit_seconds:386})({duration_ms:607_000});
assert.ok(m.includes('10:07'),`the song's own length: ${m}`);
assert.ok(m.includes('6:26'),`the cap the core reported: ${m}`);
assert.ok(/Trim it in an audio editor/.test(m),`a next step: ${m}`);

// No cap to quote: say so in words, never a number the core did not give.
for(const s of [null,undefined,{},{limit_seconds:0},{limit_seconds:'x'}]){
 const out=build(s)({duration_ms:607_000});
 assert.ok(!/\d:\d\d/.test(out),`quoted a time with no cap: ${out}`);
 assert.ok(out.includes('longer than the game fits'),out);
 assert.ok(/Trim it in an audio editor/.test(out),out);}

// A cap but no readable length: the cap still shows, the length clause goes.
const noLen=build({limit_seconds:386})({});
assert.ok(noLen.includes('6:26')&&!noLen.includes('undefined'),noLen);

// And the page holds no length of its own: the only number is the core's.
const fn=grab('tooLongProblem');
assert.ok(!/\b(380|386|6:2\d)\b/.test(fn),`a length is written into the page: ${fn}`);

console.log('PASS: the too-long sentence quotes the core\'s cap and the song\'s length, says so in words when no cap has been read, and writes no length of its own.');
