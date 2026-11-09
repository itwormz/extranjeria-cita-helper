import catalog from '../../app/desktop/assets/translations.json';
import additions from './messages.json';
export function translate(key:string,language:string):string{if(language==='ru')return key;const table=catalog[language as keyof typeof catalog] as Record<string,string>|undefined;const extra=additions[language as keyof typeof additions] as Record<string,string>|undefined;return extra?.[key]??table?.[key]??key;}
