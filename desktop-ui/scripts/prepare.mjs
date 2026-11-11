// Reproducible frontend setup. Dependencies/cache/output stay under .local/builds.
import {mkdir,copyFile,lstat,symlink,readlink} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {dirname,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const source=resolve(root,'desktop-ui');const staging=resolve(root,'.local/builds/work/rust-ui');
await mkdir(staging,{recursive:true});
for(const file of ['package.json','package-lock.json'])await copyFile(resolve(source,file),resolve(staging,file));
const link=resolve(source,'node_modules');
try{const info=await lstat(link);if(!info.isSymbolicLink()||resolve(source,await readlink(link))!==resolve(staging,'node_modules'))throw new Error('Unexpected frontend dependency path; left unchanged');}catch(error){if(error.code!=='ENOENT')throw error;await symlink('../.local/builds/work/rust-ui/node_modules',link,'dir');}
function run(command,args){const result=spawnSync(command,args,{cwd:root,stdio:'inherit'});if(result.error)throw result.error;if(result.status!==0)process.exit(result.status??1);}
run(process.platform==='win32'?'npm.cmd':'npm',['ci','--prefix',staging,'--cache',resolve(root,'.local/builds/work/npm-cache'),'--ignore-scripts','--no-fund','--no-audit']);
run(process.execPath,[resolve(staging,'node_modules/@tauri-apps/cli/tauri.js'),'icon',resolve(root,'assets/santacita-icon.svg'),'--output',resolve(root,'.local/builds/work/rust-ui-icons')]);
run(process.platform==='win32'?'npm.cmd':'npm',['run','check','--prefix',source]);run(process.platform==='win32'?'npm.cmd':'npm',['run','build','--prefix',source]);
