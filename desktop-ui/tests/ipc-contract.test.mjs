import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
test('native UI commands have an explicit local-main permission and build manifest entry',()=>{
 const root=new URL('../../crates/ceh-desktop/',import.meta.url);const source=readFileSync(new URL('src/main.rs',root),'utf8');const build=readFileSync(new URL('build.rs',root),'utf8');const capability=JSON.parse(readFileSync(new URL('capabilities/main.json',root),'utf8'));
 const commands=[...source.matchAll(/#\[tauri::command\]\s*(?:async\s+)?fn\s+(\w+)/g)].map(v=>v[1]);assert.ok(commands.includes('search_start'));
 for(const name of commands){assert.ok(build.includes(`"${name}"`),`build manifest: ${name}`);assert.ok(capability.permissions.includes(`allow-${name.replaceAll('_','-')}`),`local permission: ${name}`);}
 assert.deepEqual(capability.windows,['main']);assert.equal(capability.local,true);assert.equal(capability.remote,undefined);
});

test('every command the window invokes exists in the native shell with the argument names it sends',()=>{
 const root=new URL('../../crates/ceh-desktop/',import.meta.url);const source=readFileSync(new URL('src/main.rs',root),'utf8');const bridge=readFileSync(new URL('../src/bridge.ts',import.meta.url),'utf8');
 const native=new Map([...source.matchAll(/#\[tauri::command\]\s*(?:async\s+)?fn\s+(\w+)\s*\(([\s\S]*?)\)\s*->/g)].map(v=>[v[1],[...v[2].matchAll(/^\s*(\w+)\s*:/gm)].map(a=>a[1]).filter(name=>!['window','runtime'].includes(name))]));
 const invoked=[...bridge.matchAll(/invoke\('(\w+)'(?:,\{([^}]*)\})?\)/g)];assert.ok(invoked.length>10);
 for(const [,name,args] of invoked){
  assert.ok(native.has(name),`native command exists: ${name}`);
  const sent=(args??'').split(',').map(v=>v.trim().split(':')[0].trim()).filter(Boolean).sort();
  const expected=native.get(name).map(v=>v.replace(/_([a-z])/g,(_,c)=>c.toUpperCase())).sort();
  assert.deepEqual(sent,expected,`arguments of ${name}`);
 }
});
