// Check maintained documentation's local links without installing dependencies.
import {readFile,access} from 'node:fs/promises';import {dirname,resolve,relative} from 'node:path';
const root=resolve(import.meta.dirname,'..');
const files=['README.md','README.ru.md','README.es.md','CHANGELOG.md','docs/BUILD.md','docs/PRIVACY.md','SECURITY.md','CONTRIBUTING.md'];
let errors=0;
for(const file of files){
 const body=await readFile(resolve(root,file),'utf8');
 const links=[...body.matchAll(/!?\[[^\]]*\]\(([^\s)]+)(?:\s+[^)]*)?\)/g)].map(match=>match[1]);
 links.push(...[...body.matchAll(/(?:src|href)=["']([^"']+)["']/g)].map(match=>match[1]));
 for(const link of links){
  if(link.startsWith('#')||/^[a-z][a-z0-9+.-]*:/i.test(link))continue;
  const path=decodeURIComponent(link.split('#')[0]);if(!path)continue;
  const target=resolve(root,dirname(file),path);
  try{if(relative(root,target).startsWith('..'))throw new Error('outside repository');await access(target);}catch{console.error(`${file}: missing local link ${link}`);errors++;}
 }
}
if(errors)process.exitCode=1;else console.log(`Documentation: ${files.length} files, 0 missing local links`);
