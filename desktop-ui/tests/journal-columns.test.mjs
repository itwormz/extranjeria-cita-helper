import test from 'node:test';import assert from 'node:assert/strict';
import {selectionScope,defaultWidths,resizeWidths,loadWidths,saveWidths} from '../src/journal-columns.ts';
test('the selection scope changes only when the journal, profile or running state really change',()=>{
 // The polled snapshot is a new object every half second; a scope built from its fields must stay equal.
 const first={profile:'p1',active:true},second={profile:'p1',active:true};
 assert.equal(selectionScope('timeline',first.profile,first.active),selectionScope('timeline',second.profile,second.active));
 assert.notEqual(selectionScope('timeline','p1',true),selectionScope('offices','p1',true));
 assert.notEqual(selectionScope('timeline','p1',true),selectionScope('timeline','p2',true));
 assert.notEqual(selectionScope('timeline','p1',true),selectionScope('timeline','p1',false));
 assert.equal(selectionScope('timeline',undefined,undefined),selectionScope('timeline',null,undefined));
});
test('dragging a border resizes the column to its left and never below the minimum',()=>{
 const widths=[90,190,215];
 assert.deepEqual(resizeWidths(widths,1,30),[90,220,215]);
 assert.deepEqual(resizeWidths(widths,1,-500),[90,60,215]);
 assert.deepEqual(resizeWidths(widths,0,-100,40),[40,190,215]);
 assert.deepEqual(widths,[90,190,215],'the input is not modified');
 assert.deepEqual(resizeWidths(widths,5,10),widths,'an unknown column changes nothing');
});
test('each journal has its own default widths',()=>{
 assert.deepEqual(defaultWidths('timeline'),[90,190,215]);
 assert.deepEqual(defaultWidths('offices'),[90]);
});
test('saved widths are used only when they are sane, otherwise the defaults',()=>{
 const memory=new Map();const storage={getItem:k=>memory.has(k)?memory.get(k):null,setItem:(k,v)=>void memory.set(k,v)};
 assert.deepEqual(loadWidths('timeline',storage),[90,190,215]);
 saveWidths('timeline',[100,250,300],storage);
 assert.deepEqual(loadWidths('timeline',storage),[100,250,300]);
 memory.set('santacita.journal-columns.timeline','not json');assert.deepEqual(loadWidths('timeline',storage),[90,190,215]);
 memory.set('santacita.journal-columns.timeline',JSON.stringify([10,99999,'x']));assert.deepEqual(loadWidths('timeline',storage),[90,190,215]);
 memory.set('santacita.journal-columns.timeline',JSON.stringify([100,200]));assert.deepEqual(loadWidths('timeline',storage),[90,190,215],'wrong number of columns');
 const broken={getItem(){throw new Error('blocked');},setItem(){throw new Error('blocked');}};
 assert.deepEqual(loadWidths('timeline',broken),[90,190,215]);saveWidths('timeline',[1,2,3],broken);
});
