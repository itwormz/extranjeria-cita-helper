import test from 'node:test';import assert from 'node:assert/strict';
import {buildEntries,displayRows} from '../src/journal-view.ts';
let next=1;
const row=(stage,status,destination=null,extra={})=>({id:next++,time:1000*60*next,stage,status,event:'visit',destination,offices:[],selected_office:'',submitted:false,checked:false,applicant_continue:false,booking_details:{},deadline:null,...extra});
const ev=(event,extra={})=>({...row(null,'passed'),event,...extra});
const kinds=entries=>entries.map(e=>e.kind);
const ctx=()=>({time:ms=>`T${ms/60000}`,minute:ms=>`M${ms/60000}`,what:r=>`what#${r.id}`,t:k=>k,fill:(text,values)=>text.replace(/\{(\w+)\}/g,(_,key)=>String(values[key]??''))});
test('navigation steps and in-progress rows are not events and never reach the journal',()=>{
 next=1;
 const entries=buildEntries([row('province','processing'),row('province','passed','procedure'),row('procedure','passed','auth'),row('auth','passed','applicant_no_slots'),row('request_menu','recovering')]);
 assert.deepEqual(entries,[]);
});
test('the reported case: the reset, the new pass and the waiting after it, nothing else',()=>{
 next=1;
 const rows=[row('applicant_no_slots','passed','applicant'),row('applicant','unconfirmed','province'),row('province','passed','procedure'),row('procedure','passed','auth'),row('auth','passed','applicant_no_slots'),row('applicant_no_slots','passed','applicant_no_slots')];
 const entries=buildEntries(rows);
 assert.deepEqual(kinds(entries),['wait','pass','row']);
 assert.equal(entries[1].number,2);
 assert.equal(entries[2].row.status,'unconfirmed');
});
test('repeated no-slots cycles are one line with a count',()=>{
 next=1;
 const entries=buildEntries([row('applicant_no_slots','passed','applicant_no_slots'),row('applicant_no_slots','passed','applicant_no_slots'),row('applicant_no_slots','passed','applicant_no_slots')]);
 assert.equal(entries.length,1);
 assert.deepEqual([entries[0].kind,entries[0].count],['wait',3]);
 const shown=displayRows(entries,ctx());
 assert.equal(shown[0].cells[1],'Сит нет · проверок: 3');
 assert.equal(shown[0].icon,'clock');
});
test('one no-slots cycle reads simply "no slots"',()=>{
 next=1;
 const shown=displayRows(buildEntries([row('applicant_no_slots','passed','applicant_no_slots')]),ctx());
 assert.equal(shown[0].cells[1],'Сит нет');
});
test('results and problems stay: offices, errors, resets, a chosen office, control events',()=>{
 next=1;
 const entries=buildEntries([ev('start'),ev('flow_start'),row('office','offices_found'),row('office','passed','contact',{selected_office:'CNP'}),row('applicant','interrupted','expired'),row('expired','recovering'),row('login','login_required'),ev('pause'),ev('finish')]);
 assert.deepEqual(entries.map(e=>e.kind==='row'?e.row.status+'/'+e.row.event:e.kind),['passed/finish','passed/pause','login_required/visit','interrupted/visit','passed/visit','offices_found/visit','passed/start']);
});
test('an error of the site and a new-pass command each start a pass with their own numbering per run',()=>{
 next=1;
 const entries=buildEntries([ev('start'),row('province','passed','procedure'),row('applicant','interrupted','expired'),row('expired','recovering'),row('province','passed','procedure'),ev('flow_restart'),row('province','passed','procedure'),ev('finish'),ev('start'),row('province','passed','procedure'),row('applicant','unconfirmed','province'),row('province','passed','procedure')]);
 const passes=entries.filter(e=>e.kind==='pass').map(e=>[e.number,e.reason]).reverse();
 assert.deepEqual(passes,[[2,null],[3,'restart'],[2,null]]);
});
test('a new pass reads "Новый проход N", with "перезапуск" only when the person asked for it',()=>{
 next=1;
 const shown=displayRows(buildEntries([row('province','passed','procedure'),ev('flow_restart')]),ctx());
 assert.equal(shown[0].cells[1],'Новый проход 2 · перезапуск');
 next=1;
 const reset=displayRows(buildEntries([row('applicant','unconfirmed','province'),row('province','passed','procedure')]),ctx());
 assert.deepEqual(reset.map(r=>r.cells[1]),['Новый проход 2','what#1'],'the cause is its own line right below');
});
test('rows are two cells (time and what happened) with an icon only where it helps; every key is unique',()=>{
 next=1;
 const shown=displayRows(buildEntries([row('office','offices_found'),row('applicant','interrupted','expired'),row('applicant_no_slots','passed','applicant_no_slots'),ev('start')]),ctx());
 assert.ok(shown.every(r=>r.cells.length===2&&typeof r.key==='string'));
 assert.equal(new Set(shown.map(r=>r.key)).size,shown.length);
 assert.deepEqual(shown.map(r=>r.icon),[null,'clock','alert','office']);
 assert.deepEqual(buildEntries([]),[]);
});
