// Always refresh and type-check the frontend before embedding it in the native app.
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {dirname,resolve} from 'node:path';
const source=resolve(dirname(fileURLToPath(import.meta.url)),'..');
for(const script of ['check','build']){
 const result=spawnSync(process.platform==='win32'?'npm.cmd':'npm',['run',script,'--prefix',source],{stdio:'inherit'});
 if(result.error)throw result.error;if(result.status!==0)process.exit(result.status??1);
}
