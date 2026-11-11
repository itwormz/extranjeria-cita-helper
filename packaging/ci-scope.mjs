// Fail closed: unknown paths or unavailable comparisons always run application checks.
import {spawnSync} from 'node:child_process';import {resolve} from 'node:path';import {fileURLToPath} from 'node:url';
export function classifyPaths(files,comparisonOk=true){
 if(!comparisonOk||!files.length)return 'full';
 return files.every(file=>!file.split('/').includes('..')&&(/^(README(?:\.ru|\.es)?\.md|CHANGELOG\.md|SECURITY\.md|CONTRIBUTING\.md|LICENSE)$/.test(file)||/^docs\/[^\n]+\.(md|svg|png)$/.test(file)))?'docs':'full';
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const [base,head]=process.argv.slice(2);let mode='full';
 if(/^[a-f0-9]{40}$/.test(base??'')&&/^[a-f0-9]{40}$/.test(head??'')){
  const result=spawnSync('git',['diff','--name-only','-z',base,head,'--'],{encoding:'utf8'});
  mode=classifyPaths((result.stdout??'').split('\0').filter(Boolean),!result.error&&result.status===0);
 }
 console.log(mode);
}
