import test from 'node:test';import assert from 'node:assert/strict';
import {browserChoices} from '../src/browsers.ts';
const list=[{id:'firefox',name:'Firefox',version:'159.0',support:'supported'},{id:'chrome',name:'Google Chrome',version:'142.0',support:'supported'},{id:'edge',name:'Microsoft Edge',version:'142.0',support:'planned'},{id:'safari',name:'Safari',version:'',support:'unsupported'}];
test('the profile can only pick a supported installed browser; the others are listed as unavailable',()=>{
 const result=browserChoices(list,'firefox');
 assert.deepEqual(result.options,[{value:'firefox',label:'Firefox 159.0'},{value:'chrome',label:'Google Chrome 142.0'}]);
 assert.deepEqual(result.unavailable.map(b=>[b.name,b.support]),[['Microsoft Edge','planned'],['Safari','unsupported']]);
});
test('before the list is known, or when nothing supported is installed, the saved browser stays selectable',()=>{
 assert.deepEqual(browserChoices(null,'firefox').options,[{value:'firefox',label:'Firefox'}]);
 assert.deepEqual(browserChoices([],'firefox').options,[{value:'firefox',label:'Firefox'}]);
 assert.deepEqual(browserChoices([list[2]],'firefox').options,[{value:'firefox',label:'Firefox'}]);
});

import {fallbackBrowser} from '../src/browsers.ts';
const edgeOnly=[{id:'edge',name:'Microsoft Edge',version:'157.0.4341.46',support:'supported'}];
test('a profile whose browser is not installed falls back to the first installed one',()=>{
 assert.equal(fallbackBrowser(edgeOnly,'firefox'),'edge');
 assert.equal(fallbackBrowser(edgeOnly,'edge'),null,'an installed browser stays');
 assert.equal(fallbackBrowser([],'firefox'),null,'nothing installed: nothing to switch to');
 assert.equal(fallbackBrowser(null,'firefox'),null,'the list is still loading');
 assert.equal(fallbackBrowser([{id:'safari',name:'Safari',version:'',support:'unsupported'},...edgeOnly],'firefox'),'edge','unsupported browsers are never chosen');
});
test('a missing current browser is not offered once the installed list is known',()=>{
 assert.deepEqual(browserChoices(edgeOnly,'firefox').options.map(o=>o.value),['edge']);
 assert.deepEqual(browserChoices(null,'firefox').options.map(o=>o.value),['firefox'],'while loading the current value is shown');
 assert.deepEqual(browserChoices([],'firefox').options.map(o=>o.value),['firefox'],'no browser at all: keep the value so the note can explain');
});
