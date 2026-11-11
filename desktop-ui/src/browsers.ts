import type {InstalledBrowser} from './bridge';
const NAMES:Record<string,string>={firefox:'Firefox',chrome:'Google Chrome',brave:'Brave',edge:'Microsoft Edge'};
/** Browser choice of a profile: only supported installed browsers can be picked; the rest are listed for information. */
export function browserChoices(list:InstalledBrowser[]|null,current:string):{options:{value:string;label:string}[];unavailable:InstalledBrowser[]}{
 const options=(list??[]).filter(b=>b.support==='supported').map(b=>({value:b.id,label:b.name+(b.version?' '+b.version:'')}));
 if(!options.some(o=>o.value===current))options.unshift({value:current,label:NAMES[current]??current});
 return {options,unavailable:(list??[]).filter(b=>b.support!=='supported')};
}
