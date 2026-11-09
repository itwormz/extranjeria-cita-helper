import test from 'node:test';import assert from 'node:assert/strict';
import {guardWheel} from '../src/wheel.ts';
function field(tagName,type,focused=true){const element={tagName,type,blurred:0,blur(){this.blurred++;},ownerDocument:{}};element.ownerDocument.activeElement=focused?element:null;return element;}
function wheel(target){return {target,defaultPrevented:false,prevented:0,preventDefault(){this.prevented++;}};}

test('a wheel over the focused number field leaves the field so the page scrolls instead of changing the value',()=>{
 const input=field('INPUT','number');const event=wheel(input);
 assert.equal(guardWheel(event),true);assert.equal(input.blurred,1);assert.equal(event.prevented,0);
});
test('selects are guarded the same way and never cancel the scroll',()=>{
 const select=field('SELECT','select-one');const event=wheel(select);
 assert.equal(guardWheel(event),true);assert.equal(select.blurred,1);assert.equal(event.prevented,0);
});
test('text fields, unfocused fields and non-elements are left alone',()=>{
 for(const target of [field('INPUT','text'),field('INPUT','number',false),field('TEXTAREA','textarea'),field('DIV',''),null,{}]){
  const event=wheel(target);assert.equal(guardWheel(event),false);assert.equal(event.prevented,0);
  if(target?.blur)assert.equal(target.blurred,0);
 }
});
