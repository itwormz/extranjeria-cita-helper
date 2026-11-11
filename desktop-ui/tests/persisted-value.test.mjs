import test from 'node:test';import assert from 'node:assert/strict';import {SerialWrites} from '../src/serial.ts';import {PersistedValue} from '../src/persisted-value.ts';
test('failed rapid changes roll back to the last persisted value, never another failed optimistic value',async()=>{
 const queue=new SerialWrites();let stored='original';const controller=new PersistedValue('original',async value=>{if(value!=='saved')throw new Error('storage unavailable');stored=value;},queue);
 const a=controller.set('failed-a');const b=controller.set('failed-b');const [one,two]=await Promise.all([a,b]);assert.equal(one.ok,false);assert.equal(controller.isCurrent(one),false);assert.equal(two.ok,false);assert.equal(controller.isCurrent(two),true);assert.equal(two.value,'original');assert.equal(stored,'original');
 const three=await controller.set('saved');assert.equal(three.ok,true);assert.equal(three.value,'saved');const four=await controller.set('failed-c');assert.equal(four.value,'saved');assert.equal(stored,'saved');
});
test('preference persistence owns an immutable snapshot while the UI continues editing',async()=>{
 let release;const pending=new Promise(resolve=>release=resolve);let persisted;
 const controller=new PersistedValue({sound:false},async value=>{await pending;persisted=value;},new SerialWrites());const input={sound:true};const result=controller.set(input);input.sound=false;release();assert.equal((await result).value.sound,true);assert.equal(persisted.sound,true);
});

test('an imported preference snapshot becomes the rollback baseline',async()=>{
 const queue=new SerialWrites();const controller=new PersistedValue({beta_updates:false,sound:true},async()=>{throw new Error('save failed');},queue);
 controller.replace({beta_updates:true,sound:true});
 const result=await controller.set({beta_updates:true,sound:false});
 assert.equal(result.ok,false);assert.equal(result.value.beta_updates,true);assert.equal(result.value.sound,true);
});
