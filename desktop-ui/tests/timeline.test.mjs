import test from 'node:test';import assert from 'node:assert/strict';import {dayBounds,wallRanges,daysForRows,rowKey,zoomView,panView,tickStep,clusterMarkers,selectionForKeys,pruneSelection,piecesForDay,gestureView} from '../src/timeline.ts';
test('Madrid day geometry retains real 23/25-hour duration and does not bridge clock jumps',()=>{
 const fall=dayBounds('2026-10-25');assert.equal((fall.end-fall.start)/3600000,25);
 const spring=dayBounds('2026-03-29');assert.equal((spring.end-spring.start)/3600000,23);
 const spans=wallRanges('2026-03-29',spring.start,spring.end);assert.equal(spans.length,2);assert.equal(spans[0].right,2/24);assert.equal(spans[1].left,3/24);assert.equal(spans[1].right,1);
 const repeated=wallRanges('2026-10-25',fall.start,fall.end);assert.equal(repeated[0].right,3/24);assert.equal(repeated[1].left,2/24);assert.equal(repeated[1].right,1);
});
test('history shows observed days, not every day of a long no-observation gap',()=>{
 const rows=[{id:'a',event:false,timeline_state:'search',start:'2026-10-20T10:00:00Z',end:'2026-10-20T11:00:00Z'},{id:'g',event:false,timeline_state:'gap',start:'2026-10-20T11:00:00Z',end:'2026-10-24T10:00:00Z'},{id:'b',event:false,timeline_state:'search',start:'2026-10-24T10:00:00Z',end:'2026-10-24T11:00:00Z'}];
 assert.deepEqual(daysForRows(rows,{period:'all'}),['2026-10-20','2026-10-24']);assert.deepEqual(daysForRows(rows,{period:'range',start:'2026-10-21',end:'2026-10-23'}),[]);assert.equal(rowKey(rows[0]),'clip:a');assert.equal(rowKey({...rows[0],event:true}),'event:a');
});
test('pan and zoom preserve the anchor within bounds and never choose a clip',()=>{
 assert.deepEqual(zoomView({start:0,end:1},.5,.25),{start:.125,end:.625});assert.deepEqual(panView({start:.125,end:.625},-1),{start:0,end:.5});assert.deepEqual(panView({start:0,end:.5},1),{start:.5,end:1});assert.equal(tickStep(1),3*3600);assert.ok(tickStep(.1)<tickStep(1));
});
test('nearby event markers cluster without changing the source events',()=>{const markers=[{left:.5,row:{id:'a'}},{left:.505,row:{id:'b'}},{left:.8,row:{id:'c'}}];const value=clusterMarkers(markers,600);assert.equal(value.length,2);assert.equal(value[0].rows.length,2);assert.equal(markers.length,3);});

test('clearing or removing selected data never picks the first or last clip implicitly',()=>{
 const keys=['first','middle','last'];assert.deepEqual(pruneSelection(['gone'],keys),[]);assert.deepEqual(pruneSelection([],keys),[]);
 assert.deepEqual(selectionForKeys(keys,[],['middle'],{shift:true,anchor:'gone',additive:false}),['middle']);
 assert.deepEqual(selectionForKeys(keys,['first'],['last'],{shift:true,anchor:'first',additive:false}),keys);
 assert.deepEqual(selectionForKeys(keys,['first'],['last'],{shift:false,anchor:'first',additive:true}),['first','last']);
 assert.deepEqual(selectionForKeys(keys,['first','last'],['last'],{shift:false,anchor:'first',additive:true}),['first']);
});

test('page and action counters do not become duplicate error or manual markers',()=>{
 const base={id:'counter',kind:'processed_page',event:true,timeline_state:'recovery',start:'2026-10-20T12:00:00Z',end:'2026-10-20T12:00:00Z'};
 const markers=piecesForDay([base,{...base,id:'action',kind:'automation_action'},{...base,id:'real',kind:'server_error'}],'2026-10-20').markers;
 assert.deepEqual(markers.map(marker=>marker.row.id),['real']);
});

test('WebKit gesture scale is relative to gesture start, not compounded on every event',()=>{const initial={start:0,end:1};assert.deepEqual(gestureView(initial,2,.5),{start:.25,end:.75});assert.deepEqual(gestureView(initial,4,.5),{start:.375,end:.625});assert.deepEqual(gestureView(initial,0,.5),initial);});
