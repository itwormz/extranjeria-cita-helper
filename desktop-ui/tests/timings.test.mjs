import test from 'node:test';
import assert from 'node:assert/strict';
import {timingInputs,parseTimingInputs} from '../src/timings.ts';
test('timing editor accepts decimal comma but never converts blank or invalid input to zero',()=>{
 const values={reload_seconds:[30,60],navigation_seconds:45,expired_seconds:[0,30,60]};
 const fields=timingInputs(values);fields.reload_seconds=['31,5','65'];
 assert.deepEqual(parseTimingInputs(fields,values),{reload_seconds:[31.5,65],navigation_seconds:45,expired_seconds:[0,30,60]});
 for(const invalid of ['', 'NaN','Infinity','1e999','30abc']){fields.reload_seconds=[invalid,'60'];assert.throws(()=>parseTimingInputs(fields,values), /reload_seconds/);}
 fields.reload_seconds=['30','60'];fields.expired_seconds=['0, 30, 60'];
 assert.deepEqual(parseTimingInputs(fields,values).expired_seconds,[0,30,60]);
 fields.expired_seconds=['0,,30'];assert.throws(()=>parseTimingInputs(fields,values),/expired_seconds/);
});

test('the boolean 429-slowdown flag survives the timing editor and is never read as a number',()=>{
 const values={reload_seconds:[30,60],adaptive_reload_on_429:true};
 const fields=timingInputs(values);
 assert.deepEqual(parseTimingInputs(fields,values),values,'untouched values round-trip');
 fields.adaptive_reload_on_429=['false'];
 assert.deepEqual(parseTimingInputs(fields,values),{reload_seconds:[30,60],adaptive_reload_on_429:false});
 for(const invalid of ['','1','yes','TRUE'])assert.throws(()=>{fields.adaptive_reload_on_429=[invalid];parseTimingInputs(fields,values);},/adaptive_reload_on_429/);
});

import {timingGroups,timingTitles} from '../src/timings.ts';
import {searchRisk,protectionFields} from '../src/protection.ts';
test('timing groups and their order are those of Python 0.3.6',()=>{
 assert.deepEqual(timingGroups.map(group=>group.title),['Ввод данных','Интервалы проверки','Восстановление','Уведомления']);
 assert.deepEqual(timingGroups[0].fields.map(([key])=>key),['action_delay_seconds','typing_ms','contact_typing_ms']);
 assert.deepEqual(timingGroups[1].fields.map(([key])=>key),['bootstrap_seconds','reload_seconds','navigation_seconds']);
 assert.equal(timingGroups[2].fields.length,11);
 for(const group of timingGroups)for(const [key] of group.fields)assert.ok(timingTitles[key],`${key} has the full title shown as the tooltip`);
 assert.equal(timingTitles.typing_ms,'Между символами NIE и имени, мс');
});
test('only entering data on a no-appointments form or a disabled protection switch makes a search risky',()=>{
 const on=Object.fromEntries(protectionFields.filter(f=>f.kind==='flag').map(f=>[f.key,true]));
 assert.deepEqual(searchRisk(false,on),{risk:false,message:0});
 assert.deepEqual(searchRisk(true,on),{risk:true,message:1});
 assert.deepEqual(searchRisk(false,{...on,confirm_new_round:false}),{risk:true,message:2});
 assert.deepEqual(searchRisk(true,{...on,adaptive_reload_on_429:false}),{risk:true,message:2});
 // Spreading attempts over the day is a preference, not protection: switching it off is not a risk.
 assert.deepEqual(searchRisk(false,{...on,distribute_attempts:false}),{risk:false,message:0});
});
test('timing fields show two decimals like the Python spin boxes, with the language decimal separator, and re-parse unchanged',()=>{
 const values={action_delay_seconds:[0.3,0.8],navigation_seconds:45,expired_seconds:[0,30,60],adaptive_reload_on_429:true};
 assert.deepEqual(timingInputs(values,{decimals:2,comma:true}),{action_delay_seconds:['0,30','0,80'],navigation_seconds:['45,00'],expired_seconds:['0, 30, 60'],adaptive_reload_on_429:['true']});
 assert.deepEqual(timingInputs(values,{decimals:2,comma:false}).action_delay_seconds,['0.30','0.80']);
 assert.deepEqual(parseTimingInputs(timingInputs(values,{decimals:2,comma:true}),values),values);
 assert.deepEqual(timingInputs(values).action_delay_seconds,['0.3','0.8'],'without options the old plain format is kept');
});
