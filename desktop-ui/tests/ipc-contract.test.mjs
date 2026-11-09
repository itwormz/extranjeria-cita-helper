import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
test('native UI commands have an explicit local-main permission and build manifest entry',()=>{
 const root=new URL('../../crates/ceh-desktop/',import.meta.url);const source=readFileSync(new URL('src/main.rs',root),'utf8');const build=readFileSync(new URL('build.rs',root),'utf8');const capability=JSON.parse(readFileSync(new URL('capabilities/main.json',root),'utf8'));
 const commands=[...source.matchAll(/#\[tauri::command\]\s*(?:async\s+)?fn\s+(\w+)/g)].map(v=>v[1]);assert.ok(commands.includes('search_start'));
 for(const name of commands){assert.ok(build.includes(`"${name}"`),`build manifest: ${name}`);assert.ok(capability.permissions.includes(`allow-${name.replaceAll('_','-')}`),`local permission: ${name}`);}
 assert.deepEqual(capability.windows,['main']);assert.equal(capability.local,true);assert.equal(capability.remote,undefined);
});
