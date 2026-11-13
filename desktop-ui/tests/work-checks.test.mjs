import test from 'node:test';import assert from 'node:assert/strict';
import {workCheckTotals,madridDayKey} from '../src/statistics.ts';
test('work checks come from persisted daily statistics and never the current run',()=>{
 assert.deepEqual(workCheckTotals({today:{checks:3},all:{checks:42}}),{today:3,all:42});
 assert.deepEqual(workCheckTotals({today:{checks:0},all:{checks:42}}),{today:0,all:42});
 assert.deepEqual(workCheckTotals({today:{},all:{}}),{today:null,all:null});
 assert.deepEqual(workCheckTotals(null),{today:null,all:null});
});
test('the work statistics refresh day follows Madrid midnight, including summer time',()=>{
 assert.equal(madridDayKey(Date.parse('2026-11-12T22:59:59Z')),'2026-11-12');
 assert.equal(madridDayKey(Date.parse('2026-11-12T23:00:00Z')),'2026-11-13');
 assert.equal(madridDayKey(Date.parse('2026-07-12T22:00:00Z')),'2026-07-13');
});
