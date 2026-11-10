import test from 'node:test';import assert from 'node:assert/strict';
import {journalText,nextSelection} from '../src/journal-copy.ts';
const headers=['Время','Страница'];const rows=[['10:00:01','Провинция'],['10:00:05','Офис\tс табом'],['10:00:09','Форма\nзаявителя']];
test('the whole journal is headers plus every row, tab separated',()=>{
 assert.equal(journalText(headers,rows,[]),'Время\tСтраница\n10:00:01\tПровинция\n10:00:05\tОфис с табом\n10:00:09\tФорма заявителя');
});
test('selected rows keep the table order and ignore unknown indexes',()=>{
 assert.equal(journalText(headers,rows,[2,0,0,9,-1]),'Время\tСтраница\n10:00:01\tПровинция\n10:00:09\tФорма заявителя');
});
test('click selects, a second click clears, Cmd/Ctrl toggles and Shift extends from the anchor',()=>{
 assert.deepEqual(nextSelection([],1,null,{toggle:false,range:false}),[1]);
 assert.deepEqual(nextSelection([1],1,1,{toggle:false,range:false}),[]);
 assert.deepEqual(nextSelection([1],3,1,{toggle:true,range:false}),[1,3]);
 assert.deepEqual(nextSelection([1,3],1,1,{toggle:true,range:false}),[3]);
 assert.deepEqual(nextSelection([1],4,1,{toggle:false,range:true}),[1,2,3,4]);
 assert.deepEqual(nextSelection([4],2,4,{toggle:false,range:true}),[2,3,4]);
 assert.deepEqual(nextSelection([],2,null,{toggle:false,range:true}),[2]);
});
