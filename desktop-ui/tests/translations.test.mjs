import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync,readdirSync} from 'node:fs';
const read=path=>JSON.parse(readFileSync(new URL(path,import.meta.url),'utf8'));
const catalog=read('../../app/desktop/assets/translations.json'),additions=read('../src/messages.json');
const sources=new URL('../src/',import.meta.url);
/** Strings that are identical in every language (protocol names, brand and network names). */
const SAME=new Set(['CAPTCHA','HTTP 429','Telegram','X','Facebook','Instagram','Reddit','Webhook','TIE','ICP','cod. oper']);
const unescape=text=>text.replace(/\\(['"\\])/g,'$1');
function literals(code){
 const found=new Set();
 for(const match of code.matchAll(/(?<![\w$.])t\(\s*(?:'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)")\s*[,)]/g))found.add(unescape(match[1]??match[2]));
 for(const match of code.matchAll(/(?<![\w$.])t\([^()'"]*\?\s*'((?:[^'\\\n]|\\.)*)'\s*:\s*'((?:[^'\\\n]|\\.)*)'\s*\)/g)){found.add(unescape(match[1]));found.add(unescape(match[2]));}
 return found;
}
test('every interface string passed to t() has a real English and Spanish translation',()=>{
 const missing=[];let total=0;
 for(const file of readdirSync(sources).filter(name=>/\.(svelte|ts)$/.test(name))){
  for(const key of literals(readFileSync(new URL(file,sources),'utf8'))){
   total++;if(SAME.has(key)||!/[\p{L}]/u.test(key))continue;
   for(const language of ['en','es']){
    const value=additions[language]?.[key]??catalog[language]?.[key];
    if(typeof value!=='string'||!value.trim()||value===key)missing.push(`${language}: ${file}: ${key}`);
   }
  }
 }
 assert.ok(total>150,`only ${total} t() literals found; the extractor is broken`);
 assert.deepEqual(missing,[],`untranslated strings:\n${missing.join('\n')}`);
});
test('the extractor reads single, double quoted, escaped and ternary literals',()=>{
 const found=literals(`t('One')+t("Two")+t('It\\'s')+t(flag?'Yes':'No')+x.t('Skip')+t(dynamic)`);
 assert.deepEqual([...found].sort(),["It's",'No','One','Two','Yes']);
});
