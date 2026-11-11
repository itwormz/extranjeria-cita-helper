import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,readFile,mkdir,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {packageNames,versionKey,readMetadata,writeChecksums,validatePackages,prepareVersion,releaseNotes} from '../release.mjs';

async function fixture(t){const dir=await mkdtemp(join(tmpdir(),'santacita-release-'));t.after(()=>rm(dir,{recursive:true,force:true}));return dir;}
test('package matrix has five exact public names including portable',()=>{
 assert.deepEqual(packageNames('0.4.0-beta.1'),['SantaCita-0.4.0-beta.1-macOS-Apple-Silicon.dmg','SantaCita-0.4.0-beta.1-macOS-Intel.dmg','SantaCita-0.4.0-beta.1-windows-x64.msi','SantaCita-0.4.0-beta.1-windows-x64-portable.zip','SantaCita-0.4.0-beta.1-x86_64.AppImage']);
 assert.throws(()=>packageNames('0.4.0-beta.01'));
});
test('beta versions sort numerically and precede their stable version',()=>{
 assert(versionKey('0.4.0-beta.10')>versionKey('0.4.0-beta.2'));
 assert(versionKey('0.4.0')>versionKey('0.4.0-beta.10'));
 for(const bad of ['0.4.0-beta.0','0.4.0-rc.1','00.4.0','0.4.0/evil'])assert.throws(()=>versionKey(bad));
});
test('metadata rejects stale frontend or wrong tag before building',async t=>{
 const root=await fixture(t);await mkdir(join(root,'desktop-ui'));
 await writeFile(join(root,'Cargo.toml'),'[workspace.package]\nversion = "0.4.0-beta.1"\n');
 await writeFile(join(root,'Cargo.lock'),'[[package]]\nname = "ceh-core"\nversion = "0.4.0-beta.1"\n');
 await writeFile(join(root,'desktop-ui/package.json'),JSON.stringify({version:'0.4.0-beta.1'}));
 await writeFile(join(root,'desktop-ui/package-lock.json'),JSON.stringify({version:'0.4.0-beta.1',packages:{'':{version:'0.4.0-beta.1'}}}));
 assert.equal((await readMetadata(root,'v0.4.0-beta.1')).version,'0.4.0-beta.1');
 await assert.rejects(readMetadata(root,'v0.4.0-beta.2'));
 await writeFile(join(root,'desktop-ui/package.json'),JSON.stringify({version:'0.3.8'}));
 await assert.rejects(readMetadata(root));
});
test('checksums cover all packages and detect corruption and missing packages',async t=>{
 const dir=await fixture(t);const names=packageNames('0.4.0-beta.1');
 for(const name of names)await writeFile(join(dir,name),'synthetic package');
 await writeChecksums(dir,names);
 const sums=await readFile(join(dir,'SHA256SUMS'),'utf8');
 const sha=createHash('sha256').update('synthetic package').digest('hex');
 assert.equal(sums.split('\n')[0],sha+'  '+names[0]);
 await validatePackages(dir,'0.4.0-beta.1',{proofs:false});
 await assert.rejects(validatePackages(dir,'0.4.0-beta.1',{proofs:true}));
 await writeFile(join(dir,names[0]),'tampered');
 await assert.rejects(validatePackages(dir,'0.4.0-beta.1',{proofs:false}));
 await rm(join(dir,names[0]));
 await assert.rejects(validatePackages(dir,'0.4.0-beta.1',{proofs:false}));
});

test('release preparation synchronizes source and notes and is safe to repeat',async t=>{
 const root=await fixture(t);await mkdir(join(root,'desktop-ui'));await mkdir(join(root,'.github'));
 await writeFile(join(root,'.github/release-version'),'0.4.0-beta.2\n');
 await writeFile(join(root,'Cargo.toml'),'[workspace.package]\nversion = "0.4.0-beta.1"\n');
 await writeFile(join(root,'Cargo.lock'),'[[package]]\nname = "ceh-core"\nversion = "0.4.0-beta.1"\n');
 await writeFile(join(root,'desktop-ui/package.json'),JSON.stringify({version:'0.4.0-beta.1'}));
 await writeFile(join(root,'desktop-ui/package-lock.json'),JSON.stringify({version:'0.4.0-beta.1',packages:{'':{version:'0.4.0-beta.1'}}}));
 await writeFile(join(root,'CHANGELOG.md'),'# Changelog\n\n## Unreleased — Beta fixes\n\n- Keep imported profiles.\n\n## 0.3.8 — Python bridge\n\n- Prepare updates.\n');
 await prepareVersion(root);
 assert.equal((await readMetadata(root)).version,'0.4.0-beta.2');
 assert.deepEqual(await releaseNotes(root,'0.4.0-beta.2'),{title:'Beta fixes',body:'- Keep imported profiles.'});
 const before=await readFile(join(root,'CHANGELOG.md'),'utf8');await prepareVersion(root);assert.equal(await readFile(join(root,'CHANGELOG.md'),'utf8'),before);
 await writeFile(join(root,'.github/release-version'),'0.4.0-beta.1\n');await assert.rejects(prepareVersion(root));
});
