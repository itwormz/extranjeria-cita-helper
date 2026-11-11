import test from 'node:test';import assert from 'node:assert/strict';
import {buildEntries} from '../src/journal-view.ts';
let next=1;
const row=(stage,status,destination=null,extra={})=>({id:next++,time:1000*next,stage,status,event:'visit',destination,offices:[],selected_office:'',submitted:false,checked:false,applicant_continue:false,booking_details:{},deadline:null,...extra});
const ev=(event,extra={})=>({...row(null,'passed'),event,...extra});
const kinds=entries=>entries.map(e=>e.kind);
test('the reported case: a reset to the start is one visible boundary with its reason, and the pass collapses into one line',()=>{
 next=1;
 const rows=[
  row('applicant_no_slots','passed','applicant'),
  row('applicant','unconfirmed','province'),
  row('province','passed','procedure'),
  row('procedure','passed','auth'),
  row('auth','passed','applicant_no_slots'),
  row('applicant_no_slots','passed','applicant_no_slots',{deadline:5000}),
 ];
 const entries=buildEntries(rows); // newest first
 assert.deepEqual(kinds(entries),['wait','run','pass','row','row']);
 const [wait,run,pass,cause,before]=entries;
 assert.equal(wait.count,1);
 assert.deepEqual(run.stages,['province','procedure','auth','applicant_no_slots']);
 assert.deepEqual([pass.number,pass.reason,pass.destination],[2,'site_reset','province']);
 assert.equal(cause.row.status,'unconfirmed');
 assert.equal(before.row.stage,'applicant_no_slots');
});
test('repeated no-slots cycles collapse into one entry with a count and the last known next check',()=>{
 next=1;
 const rows=[row('applicant_no_slots','passed','applicant_no_slots',{deadline:1}),row('applicant_no_slots','passed','applicant_no_slots',{deadline:2}),row('applicant_no_slots','passed','applicant_no_slots',{deadline:3})];
 const entries=buildEntries(rows);
 assert.equal(entries.length,1);
 assert.deepEqual([entries[0].kind,entries[0].count,entries[0].deadline],['wait',3,3]);
 assert.deepEqual(entries[0].rows.length,3,'the hidden rows stay available for expanding');
});
test('a single step stays an ordinary row',()=>{
 next=1;
 const entries=buildEntries([row('province','passed','procedure')]);
 assert.deepEqual(kinds(entries),['row']);
});
test('an error of the site starts a new pass with that reason, and an explicit new-pass event is a boundary too',()=>{
 next=1;
 const entries=buildEntries([row('applicant','interrupted','expired'),row('expired','recovering'),row('province','passed','procedure')]);
 assert.deepEqual(kinds(entries),['row','pass','row','row']);
 assert.equal(entries[1].reason,'error');
 next=1;
 const explicit=buildEntries([ev('start'),row('province','passed','procedure'),ev('flow_restart'),row('province','passed','procedure')]);
 assert.deepEqual(kinds(explicit),['row','pass','row','row'],'the new-pass event itself is replaced by the boundary');
 assert.deepEqual([explicit[1].number,explicit[1].reason],[2,'restart']);
});
test('each run of the search numbers its passes from one',()=>{
 next=1;
 const entries=buildEntries([ev('start'),row('province','passed','procedure'),row('applicant','unconfirmed','province'),row('province','passed','procedure'),ev('finish'),ev('start'),row('province','passed','procedure'),row('applicant','unconfirmed','province'),row('province','passed','procedure')]);
 const numbers=entries.filter(e=>e.kind==='pass').map(e=>e.number).reverse();
 assert.deepEqual(numbers,[2,2]);
});
test('rows without a stage or from the control events are never merged',()=>{
 next=1;
 const entries=buildEntries([ev('start'),ev('pause'),ev('resume')]);
 assert.deepEqual(kinds(entries),['row','row','row']);
 assert.deepEqual(buildEntries([]),[]);
});

import {displayRows} from '../src/journal-view.ts';
const ctx=(extra={})=>({
 time:ms=>`T${ms/1000}`,minute:ms=>`M${Math.floor(ms/1000/60)}`,clock:ms=>`C${ms}`,
 page:stage=>stage?`[${stage}]`:'',what:row=>`what#${row.id}`,t:k=>k,
 fill:(text,values)=>text.replace(/\{(\w+)\}/g,(_,key)=>String(values[key]??'')),expanded:[],...extra});
test('the reported case reads as five short lines with the boundary and its reason',()=>{
 next=1;
 const rows=[row('applicant_no_slots','passed','applicant'),row('applicant','unconfirmed','province'),row('province','passed','procedure'),row('procedure','passed','auth'),row('auth','passed','applicant_no_slots'),row('applicant_no_slots','passed','applicant_no_slots',{deadline:5000})];
 const shown=displayRows(buildEntries(rows),ctx());
 assert.deepEqual(shown.map(r=>r.kind),['wait','run','pass','row','row']);
 assert.equal(shown[0].cells[2],'Сит нет · следующая проверка в {deadline}'.replace('{deadline}','C5000'));
 assert.deepEqual(shown[1].cells.slice(1),['[province]','→ [procedure] → [auth] → [applicant_no_slots]']);
 assert.equal(shown[2].cells[2],'Новый проход 2 · сайт вернул на «[province]»');
 assert.equal(shown[3].icon,'alert','the row that explains the reset is marked');
});
test('a collapsed run opens into its own rows and closes again',()=>{
 next=1;
 const rows=[row('applicant_no_slots','passed','applicant_no_slots',{deadline:1}),row('applicant_no_slots','passed','applicant_no_slots',{deadline:2})];
 const entries=buildEntries(rows);
 const closed=displayRows(entries,ctx());
 assert.deepEqual([closed.length,closed[0].expandable,closed[0].open],[1,true,false]);
 assert.ok(closed[0].cells[2].startsWith('Сит нет · проверок: 2'));
 const open=displayRows(entries,ctx({expanded:[entries[0].id]}));
 assert.deepEqual(open.map(r=>r.kind),['wait','child','child']);
 assert.equal(open[0].open,true);
});
test('a single row is not expandable and has no icon unless it is a warning',()=>{
 next=1;
 const shown=displayRows(buildEntries([row('province','passed','procedure'),row('applicant','interrupted','expired')]),ctx());
 assert.deepEqual(shown.map(r=>[r.expandable,r.icon]),[[false,'alert'],[false,null]]);
});
