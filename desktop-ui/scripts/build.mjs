// Always refresh and type-check the frontend before embedding it in the native app.
import {runNpm} from './npm.mjs';
import {fileURLToPath} from 'node:url';
import {dirname,resolve} from 'node:path';
const source=resolve(dirname(fileURLToPath(import.meta.url)),'..');
for(const script of ['check','build']){
 const result=runNpm(['run',script,'--prefix',source]);
 if(result.error)throw result.error;if(result.status!==0)process.exit(result.status??1);
}
