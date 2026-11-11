import type {InstalledBrowser} from './bridge';
const NAMES:Record<string,string>={firefox:'Firefox',chrome:'Google Chrome',brave:'Brave',edge:'Microsoft Edge'};
/** Browser choice of a profile: only supported installed browsers can be picked; the rest are listed for information. */
export function browserChoices(list:InstalledBrowser[]|null,current:string):{options:{value:string;label:string}[];unavailable:InstalledBrowser[]}{
 const options=(list??[]).filter(b=>b.support==='supported').map(b=>({value:b.id,label:b.name+(b.version?' '+b.version:'')}));
 // The saved browser is shown only while nothing installed can replace it (list still loading, or none found).
 if(!options.length)options.unshift({value:current,label:NAMES[current]??current});
 return {options,unavailable:(list??[]).filter(b=>b.support!=='supported')};
}
/** The first installed supported browser when the profile's own is not among them; null when nothing needs changing. */
export function fallbackBrowser(list:InstalledBrowser[]|null,current:string):string|null{
 const installed=(list??[]).filter(b=>b.support==='supported');
 if(!installed.length||installed.some(b=>b.id===current))return null;
 return installed[0].id;
}
export function browserName(id:string):string{return NAMES[id]??id;}
