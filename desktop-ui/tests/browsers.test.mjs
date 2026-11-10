import test from 'node:test';import assert from 'node:assert/strict';
import {browserChoices} from '../src/browsers.ts';
const list=[{id:'firefox',name:'Firefox',version:'159.0',support:'supported'},{id:'chrome',name:'Google Chrome',version:'142.0',support:'planned'},{id:'safari',name:'Safari',version:'',support:'unsupported'}];
test('the profile can only pick a supported installed browser; the others are listed as unavailable',()=>{
 const result=browserChoices(list,'firefox');
 assert.deepEqual(result.options,[{value:'firefox',label:'Firefox 159.0'}]);
 assert.deepEqual(result.unavailable.map(b=>[b.name,b.support]),[['Google Chrome','planned'],['Safari','unsupported']]);
});
test('before the list is known, or when nothing supported is installed, the saved browser stays selectable',()=>{
 assert.deepEqual(browserChoices(null,'firefox').options,[{value:'firefox',label:'Firefox'}]);
 assert.deepEqual(browserChoices([],'firefox').options,[{value:'firefox',label:'Firefox'}]);
 assert.deepEqual(browserChoices([list[1]],'firefox').options,[{value:'firefox',label:'Firefox'}]);
});
