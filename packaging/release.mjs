// Shared release metadata and artifact checks. Node standard library only.
import {readFile,writeFile,readdir,lstat,copyFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {createReadStream} from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
export const ROOT=resolve(dirname(fileURLToPath(import.meta.url)),'..');
export function versionKey(value){
 const match=/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-beta\.([1-9][0-9]*))?$/.exec(value);
 if(!match)throw new Error('Expected X.Y.Z or X.Y.Z-beta.N');
 const [,major,minor,patch,beta]=match;const parts=[major,minor,patch,beta??'0'].map(BigInt);
 if(parts.some(n=>n>0xffffffffn))throw new Error('Version component too large');
 return (parts[0]<<128n)|(parts[1]<<96n)|(parts[2]<<64n)|(beta?parts[3]:1n<<32n);
}
export function packageNames(version,platform,arch){
 versionKey(version);
 const prefix=`SantaCita-${version}`;
 const matrix={
  'macos-arm64':[`${prefix}-macOS-Apple-Silicon.dmg`],
  'macos-x64':[`${prefix}-macOS-Intel.dmg`],
  'windows-x64':[`${prefix}-windows-x64.msi`,`${prefix}-windows-x64-portable.zip`],
  'linux-x64':[`${prefix}-x86_64.AppImage`]
 };
 if(!platform)return Object.values(matrix).flat();
 if(!matrix[`${platform}-${arch}`])throw new Error('Unsupported package target');
 return matrix[`${platform}-${arch}`];
}
export function assertNativeTarget(platform,arch,hostOS,rustHost){
 const hosts={'macos-arm64':'aarch64-apple-darwin','macos-x64':'x86_64-apple-darwin','windows-x64':'x86_64-pc-windows-msvc','linux-x64':'x86_64-unknown-linux-gnu'};
 if({macos:'darwin',windows:'win32',linux:'linux'}[platform]!==hostOS||hosts[`${platform}-${arch}`]!==rustHost)throw new Error('Package target must match the native Rust host');
 return rustHost;
}
export async function readMetadata(root=ROOT,tag){
 const cargo=await readFile(join(root,'Cargo.toml'),'utf8');
 const version=/^\[workspace.package\]\s*\nversion = "([^"]+)"/m.exec(cargo)?.[1];versionKey(version);
 if(tag&&tag!==`v${version}`)throw new Error('Tag does not match source version');
 if(tag&&(await readFile(join(root,'.github/release-version'),'utf8')).trim()!==version)throw new Error('Declared release version differs from tag');
 const packageJson=JSON.parse(await readFile(join(root,'desktop-ui/package.json'),'utf8'));
 const lock=JSON.parse(await readFile(join(root,'desktop-ui/package-lock.json'),'utf8'));
 if(packageJson.version!==version||lock.version!==version||lock.packages?.['']?.version!==version)throw new Error('Frontend version mismatch');
 const cargoLock=(await readFile(join(root,'Cargo.lock'),'utf8')).replace(/\r\n/g,'\n');
 const crates=[...cargoLock.matchAll(/^name = "(ceh-[^"]+)"\nversion = "([^"]+)"/gm)];
 if(!crates.length||crates.some(([,name,v])=>v!==version))throw new Error('Cargo.lock workspace version mismatch');
 return {version,prerelease:version.includes('-beta.')};
}
export async function sha256(path){
 const hash=createHash('sha256');for await(const chunk of createReadStream(path))hash.update(chunk);return hash.digest('hex');
}
export async function writeChecksums(directory,names,file='SHA256SUMS'){
 const rows=[];
 for(const name of names){
  if(!name||name.includes('/')||name.includes('\\'))throw new Error('Invalid artifact name');
  const stat=await lstat(join(directory,name));if(!stat.isFile()||!stat.size)throw new Error('Missing or empty package');
  rows.push(`${await sha256(join(directory,name))}  ${name}`);
 }
 await writeFile(join(directory,file),rows.join('\n')+'\n');
}
export async function validatePackages(directory,version,{proofs=true}={}){
 const names=packageNames(version);const sums=await readFile(join(directory,'SHA256SUMS'),'utf8');
 const rows=sums.trimEnd().split('\n').map(line=>/^([a-f0-9]{64})  (\S+)$/.exec(line));
 if(rows.some(row=>!row)||rows.length!==names.length)throw new Error('Invalid checksum manifest');
 for(const name of names){
  const found=rows.filter(row=>row[2]===name);
  const stat=await lstat(join(directory,name));
  if(!stat.isFile()||!stat.size||found.length!==1||found[0][1]!==await sha256(join(directory,name)))throw new Error(`Invalid package: ${name}`);
  if(proofs){const proof=await lstat(join(directory,`${name}.sigstore.json`));if(!proof.isFile()||!proof.size)throw new Error(`Missing provenance: ${name}`);}
 }
 return names;
}
export async function attachProof(directory,version,platform,arch,bundle){
 // Every package from this job shares the multi-subject bundle emitted by actions/attest.
 JSON.parse(await readFile(bundle,'utf8'));
 for(const name of packageNames(version,platform,arch))await copyFile(bundle,join(directory,`${name}.sigstore.json`));
}
export async function finalize(directory,version,{proofs=true}={}){
 const names=packageNames(version);
 const manifests=(await readdir(directory)).filter(name=>name.startsWith('SHA256SUMS-'));
 const expected=new Map();
 for(const file of manifests){
  for(const line of (await readFile(join(directory,file),'utf8')).trimEnd().split('\n')){
   const row=/^([a-f0-9]{64})  (\S+)$/.exec(line);
   if(!row||!names.includes(row[2])||expected.has(row[2]))throw new Error('Invalid platform checksum manifests');
   expected.set(row[2],row[1]);
  }
 }
 if(expected.size!==names.length)throw new Error('Incomplete package matrix');
 for(const name of names)if(expected.get(name)!==await sha256(join(directory,name)))throw new Error('Package changed after native build');
 await writeChecksums(directory,names);return validatePackages(directory,version,{proofs});
}
export async function verifyPublishedRelease(release,directory,version){
 const files=packageNames(version).flatMap(name=>[name,`${name}.sigstore.json`]).concat('SHA256SUMS');
 if(release.tag_name!==`v${version}`||release.draft!==false||release.immutable!==true||release.prerelease!==version.includes('-beta.'))throw new Error('Published release state is not immutable or has the wrong channel');
 if(!Array.isArray(release.assets)||release.assets.length!==files.length)throw new Error('Published asset set is incomplete');
 for(const name of files){
  const found=release.assets.filter(asset=>asset.name===name);
  if(found.length!==1||found[0].state!=='uploaded'||found[0].digest!==`sha256:${await sha256(join(directory,name))}`)throw new Error(`Published asset digest mismatch: ${name}`);
 }
}
export async function releaseNotes(root,version){
 versionKey(version);const notes=(await readFile(join(root,'CHANGELOG.md'),'utf8')).replace(/\r\n/g,'\n');
 const headings=[...notes.matchAll(/^## ([^\n]+)$/gm)];const heading=headings.find(h=>h[1].startsWith(`${version} — `));
 if(!heading)throw new Error('Missing release notes');
 const end=headings.find(h=>h.index>heading.index)?.index??notes.length;
 const body=notes.slice(heading.index+heading[0].length,end).trim();
 if(!body)throw new Error('Empty release notes');
 return {title:heading[1].slice(version.length+3),body};
}
export async function prepareVersion(root=ROOT){
 const requested=(await readFile(join(root,'.github/release-version'),'utf8')).trim();versionKey(requested);
 const current=(await readMetadata(root)).version;
 if(versionKey(requested)<versionKey(current))throw new Error('Release version must not go backwards');
 let notes=(await readFile(join(root,'CHANGELOG.md'),'utf8')).replace(/\r\n/g,'\n');
 if(requested===current){try{await releaseNotes(root,requested);return;}catch{/* first release may already have the intended source version */}}
 const marker=/^## Unreleased(?: — ([^\n]+))?$/m.exec(notes);
 if(!marker)throw new Error('Add final notes under Unreleased');
 const start=marker.index+marker[0].length;const next=/^## /m.exec(notes.slice(start));const end=next?start+next.index:notes.length;
 const body=notes.slice(start,end).trim();if(!body)throw new Error('Empty unreleased notes');
 if(notes.split('\n').some(line=>line.startsWith(`## ${requested} — `)))throw new Error('Release notes already exist');
 notes=notes.slice(0,marker.index)+`## Unreleased\n\n## ${requested} — ${marker[1]??'Release'}\n\n${body}\n\n`+notes.slice(end);
 const cargoPath=join(root,'Cargo.toml');const cargo=(await readFile(cargoPath,'utf8')).replace(/(\[workspace.package\]\s*\nversion = ")[^"]+"/,`$1${requested}"`);
 const lockPath=join(root,'Cargo.lock');const lock=(await readFile(lockPath,'utf8')).replace(/\r\n/g,'\n').replace(/(^name = "ceh-[^"]+"\nversion = ")[^"]+"/gm,`$1${requested}"`);
 const packagePath=join(root,'desktop-ui/package.json');const pkg=JSON.parse(await readFile(packagePath,'utf8'));pkg.version=requested;
 const npmPath=join(root,'desktop-ui/package-lock.json');const npm=JSON.parse(await readFile(npmPath,'utf8'));npm.version=requested;npm.packages[''].version=requested;
 for(const [file,content] of [[cargoPath,cargo],[lockPath,lock],[packagePath,JSON.stringify(pkg,null,2)+'\n'],[npmPath,JSON.stringify(npm,null,2)+'\n'],[join(root,'CHANGELOG.md'),notes]])await writeFile(file,content);
 await readMetadata(root);await releaseNotes(root,requested);
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const [command,...args]=process.argv.slice(2);const {version}=await readMetadata(ROOT,process.env.RELEASE_TAG||undefined);
 if(command==='prepare')await prepareVersion();
 else if(command==='notes'){const notes=await releaseNotes(ROOT,version);await writeFile(resolve(args[0]),notes.body+'\n');}
 else if(command==='metadata')console.log(JSON.stringify({version}));
 else if(command==='checksums'){const [platform,arch]=args;await writeChecksums(join(ROOT,'.local/builds/releases'),packageNames(version,platform,arch),`SHA256SUMS-${platform}-${arch}`);}
 else if(command==='attach-proof')await attachProof(join(ROOT,'.local/builds/releases'),version,...args);
 else if(command==='finalize')await finalize(resolve(args[0]),version,{proofs:args[1]!=='--no-proofs'});
 else throw new Error('Unknown release command');
}
