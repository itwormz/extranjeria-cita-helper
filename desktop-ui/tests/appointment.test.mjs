import test from 'node:test';import assert from 'node:assert/strict';import {maskTime,timeValue,splitOffice,appointmentInput,achievementValue,searchDays} from '../src/appointment.ts';
import {recordForEditing} from '../src/appointment.ts';
test('editing starts from a record in the newly loaded context, never a stale profile record or new entry',()=>{
 const first={id:'first'},latest={id:'latest'};const records=[first,latest];
 assert.equal(recordForEditing(records),latest);
 assert.equal(recordForEditing(records,'first'),first);
 assert.equal(recordForEditing(records,'other-profile'),latest);
 assert.equal(recordForEditing([],'first'),null);
});
test('appointment time separator is immediate and invalid clocks never submit',()=>{assert.equal(maskTime('09'),'09:');assert.equal(maskTime('0905'),'09:05');assert.equal(timeValue('09:05'),'09:05');for(const t of ['24:00','09:60','9:05','09:',''])assert.equal(timeValue(t),null);});
test('office selection splits only the first comma and saved private notes win over defaults',()=>{assert.deepEqual(splitOffice(' Office, Street, 12, Town '),{office:'Office',address:'Street, 12, Town'});const context={today:'2026-10-31',selected_office:'Office, Street, 12',offices:[],records:[]};assert.deepEqual(appointmentInput(context),{date:'31.10.2026',time:'09:00',office:'Office',address:'Street, 12',confirmation:''});const saved={date:'2026-11-02',time:'11:15',office:'Custom',address:'Address',confirmation:'PRIVATE',timezone:'Europe/Madrid'};assert.equal(appointmentInput(context,saved).confirmation,'PRIVATE');assert.equal(appointmentInput(context,saved).office,'Custom');});
test('achievement counters use stored immutable values including unknown effort and elapsed search days',()=>{assert.equal(achievementValue({actions_performed:128},'actions_performed'),128);assert.equal(achievementValue({actions_performed:null},'actions_performed'),null);assert.equal(achievementValue({checks:-1},'checks'),null);assert.equal(searchDays({recorded_at:'2026-11-02T12:00:00Z',statistics:{started_at:'2026-10-31T12:00:00Z'}}),2);assert.equal(searchDays({recorded_at:'2026-11-02T12:00:00Z',statistics:{}}),null);});

test('unknown historical zero remains unknown after immutable achievement storage',()=>{assert.equal(achievementValue({blocked:0},'blocked',{unknown_fields:['blocked']}),null);assert.equal(achievementValue({checks:48},'checks',{unknown_fields:['checks']}),48);});
