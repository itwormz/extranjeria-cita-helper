import test from 'node:test';import assert from 'node:assert/strict';
import {preventImplicitProfileSubmit} from '../src/profile-form.ts';
test('Enter in ordinary fields does not save, while explicit button activation and composing stay untouched',()=>{
 for(const [tagName,key,isComposing,blocked] of [['INPUT','Enter',false,true],['SELECT','Enter',false,true],['TEXTAREA','Enter',false,false],['BUTTON','Enter',false,false],['INPUT','Enter',true,false],['INPUT','a',false,false]]){
  let prevented=false;preventImplicitProfileSubmit({key,isComposing,target:{tagName},preventDefault(){prevented=true;}});
  assert.equal(prevented,blocked,`${tagName}/${key}/${isComposing}`);
 }
});
