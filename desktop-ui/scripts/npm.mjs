// Run npm without a shell. Node refuses to spawn `npm.cmd` directly on Windows (EINVAL since the 2024 security
// fix), so there npm's own script is started with the running Node instead.
import {spawnSync} from 'node:child_process';
import {existsSync} from 'node:fs';
import {dirname,resolve} from 'node:path';
export function runNpm(args,options={stdio:'inherit'}){
 const script=resolve(dirname(process.execPath),'node_modules','npm','bin','npm-cli.js');
 if(existsSync(script))return spawnSync(process.execPath,[script,...args],options);
 return spawnSync('npm',args,options);
}
