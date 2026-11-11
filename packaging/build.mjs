// Build native release packages on their target operating system; never publish.
import {spawnSync} from 'node:child_process';
import {mkdir,readdir,copyFile,chmod} from 'node:fs/promises';
import {join} from 'node:path';
import {ROOT,readMetadata,packageNames,writeChecksums,assertNativeTarget} from './release.mjs';
function run(command,args,options={}){const result=spawnSync(command,args,{stdio:'inherit',cwd:ROOT,...options});if(result.error)throw result.error;if(result.status!==0)throw new Error(`${command} failed (${result.status})`);}
const [platform,arch]=process.argv.slice(2);
const compiler=spawnSync('rustc',['-vV'],{encoding:'utf8',cwd:ROOT});
if(compiler.error||compiler.status!==0)throw new Error('Cannot identify native Rust toolchain');
const rustHost=/^host: (.+)$/m.exec(compiler.stdout)?.[1]?.trim();
assertNativeTarget(platform,arch,process.platform,rustHost);
const {version}=await readMetadata(ROOT,process.env.RELEASE_TAG||undefined);
const names=packageNames(version,platform,arch);const output=join(ROOT,'.local/builds/releases');await mkdir(output,{recursive:true});
const target=join(ROOT,'.local/builds/rust-target',rustHost,'release');
const cli=join(ROOT,'.local/builds/work/rust-ui/node_modules/@tauri-apps/cli/tauri.js');
// Tauri's build defaults to Cargo's release profile; --debug is deliberately absent.
run(process.execPath,[cli,'build','--features','shell','--ci','--target',rustHost,...(platform==='windows'?['--no-bundle']:['--bundles',platform==='macos'?'dmg':'appimage']),'--','--locked'],{cwd:join(ROOT,'crates/ceh-desktop')});
if(platform==='macos'){
 const dmg=join(target,'bundle/dmg');
 run('bash',[join(ROOT,'packaging/macos/make-dmg.sh'),dmg,join(target,'bundle/macos/SantaCita.app'),join(ROOT,'LICENSE'),join(ROOT,'assets/dmg-background.png'),join(output,names[0])]);
 run('codesign',['--verify','--deep','--strict',join(target,'bundle/macos/SantaCita.app')]);
}else if(platform==='windows'){
 run('pwsh',['-NoProfile','-File',join(ROOT,'packaging/windows/package.ps1'),'-Exe',join(target,'ceh-desktop.exe'),'-License',join(ROOT,'LICENSE'),'-Icon',join(ROOT,'.local/builds/work/rust-ui-icons/icon.ico'),'-Version',version,'-Out',output]);
}else{
 const images=(await readdir(join(target,'bundle/appimage'))).filter(name=>name.endsWith('.AppImage'));
 if(images.length!==1)throw new Error('Expected one native AppImage');
 await copyFile(join(target,'bundle/appimage',images[0]),join(output,names[0]));await chmod(join(output,names[0]),0o755);
}
await writeChecksums(output,names,`SHA256SUMS-${platform}-${arch}`);
