// Called only by a tag build after every platform succeeded. Never imported by the desktop app.
import {spawnSync} from 'node:child_process';
import {join,resolve} from 'node:path';
import {writeFile} from 'node:fs/promises';
import {ROOT,readMetadata,releaseNotes,finalize} from './release.mjs';
function run(command,args,options={}){const r=spawnSync(command,args,{cwd:ROOT,stdio:'inherit',...options});if(r.error)throw r.error;if(r.status!==0)throw new Error(`${command} failed (${r.status})`);return r;}
const repository=process.env.GITHUB_REPOSITORY;const commit=process.env.GITHUB_SHA;const tag=process.env.RELEASE_TAG;
if(!['itwormz/extranjeria-cita-helper','itwormz/santacita'].includes(repository)||!/^[a-f0-9]{40}$/.test(commit??''))throw new Error('Unexpected release source');
if(process.env.GITHUB_EVENT_NAME!=='push'||process.env.GITHUB_REF!==`refs/tags/${tag}`)throw new Error('Publication requires a pushed release tag');
const {version,prerelease}=await readMetadata(ROOT,tag);const directory=resolve(process.argv[2]);
const names=await finalize(directory,version);const notes=await releaseNotes(ROOT,version);
// Reuse the native application's exact cryptographic policy, not a second weaker publisher check.
for(const name of names)run('cargo',['run','--locked','-p','ceh-services','--example','verify_provenance','--',join(directory,name),version,`https://github.com/${repository}`,commit]);
const existing=spawnSync('gh',['api',`repos/${repository}/releases/tags/${tag}`],{cwd:ROOT,encoding:'utf8'});
if(existing.error)throw existing.error;
if(existing.status===0)throw new Error('Release already exists; published packages must never be overwritten. Inspect any existing draft before rerunning.');
if(!existing.stderr.includes('HTTP 404'))throw new Error('Cannot determine whether release exists');
const notesFile=join(directory,'release-notes.md');await writeFile(notesFile,notes.body+'\n');
run('gh',['release','create',tag,'--repo',repository,'--verify-tag','--target',commit,'--draft','--title',`SantaCita ${version} — ${notes.title}`,'--notes-file',notesFile,...(prerelease?['--prerelease']:[])]);
const assets=names.flatMap(name=>[join(directory,name),join(directory,`${name}.sigstore.json`)]).concat(join(directory,'SHA256SUMS'));
run('gh',['release','upload',tag,'--repo',repository,...assets]);
run('gh',['release','edit',tag,'--repo',repository,'--draft=false',...(prerelease?['--prerelease','--latest=false']:['--latest'])]);
