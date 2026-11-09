import test from 'node:test';import assert from 'node:assert/strict';
import {escapePoint,proverb} from '../src/easter-egg.ts';
import {readFileSync} from 'node:fs';
test('escape destination remains inside the board, including compact sizes',()=>{
 for(const [w,h] of [[524,300],[260,180],[48,48]]){const p=escapePoint(w,h,{x:w/2,y:h/2},()=>.75);assert.ok(p.x>=0&&p.x<=Math.max(0,w-56));assert.ok(p.y>=0&&p.y<=Math.max(0,h-56));}
});
test('keeps fifteen original proverbs per language and a safe fallback',()=>{
 const lists=JSON.parse(readFileSync(new URL('../src/proverbs.json',import.meta.url)));
 for(const lang of ['ru','en','es']){assert.equal(lists[lang].length,15);assert.equal(new Set(lists[lang]).size,15);assert.equal(proverb(lang,()=>0),lists[lang][0]);}
 assert.equal(proverb('unknown',()=>0),lists.en[0]);
});
