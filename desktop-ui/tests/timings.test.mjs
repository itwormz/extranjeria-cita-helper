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
