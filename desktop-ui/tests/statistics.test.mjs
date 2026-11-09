import test from 'node:test';
import assert from 'node:assert/strict';
import {metricValue,formatDuration,firstEvent,stamp} from '../src/statistics.ts';
const t=key=>key;
test('legacy unknown zero differs from observed zero and positive observations survive',()=>{
 const legacy={checks:42,active_seconds:0,days:0,server_errors:0,actions_performed:2,legacy_unknown_fields:['active_seconds','server_errors','started_at','actions_performed']};
 assert.equal(metricValue(legacy,'checks'),42);
 assert.equal(metricValue(legacy,'server_errors'),null);
 assert.equal(metricValue(legacy,'active_seconds'),null);
 assert.equal(metricValue(legacy,'days'),null);
 assert.equal(metricValue(legacy,'actions_performed'),null);
 assert.equal(metricValue({...legacy,effort_started_at:'2026-10-31T10:00:00Z'},'actions_performed'),2);
 assert.equal(metricValue({server_errors:0},'server_errors'),0);
 assert.equal(metricValue({days:0,checks:42},'days'),null);
 assert.equal(metricValue({active_seconds:Infinity},'active_seconds'),null);
 assert.equal(metricValue({checks:-1},'checks'),null);
});
test('duration is compact and first facts preserve precision and evidence quality',()=>{
 assert.equal(formatDuration(3600,'ru'),'1 ч');assert.equal(formatDuration(0,'ru'),'0 мин');assert.equal(formatDuration(3660,'ru'),'1 ч 1 мин');assert.equal(formatDuration(null,'ru'),'—');
 const data={first_office_at:'2026-10-31T10:12:13Z',first_office_seconds:40,first_office_status:'recorded',first_office_active_quality:'minimum'};
 const fact=firstEvent(data,'first_office','all','ru',t);
 assert.equal(fact.caption,'Первый офис в истории');assert.match(fact.value,/11:12:13/);assert.match(fact.value,/≥ 40 с поиска/);assert.match(fact.hint,/пропуски/);
 assert.equal(firstEvent({first_booking_status:'not_seen'},'first_booking','today','ru',t).value,'Ещё не появлялись');
 assert.equal(firstEvent({first_booking_status:'unknown'},'first_booking','all','ru',t).value,'Дата неизвестна');
});

test('timestamps retain a four-digit year and 24-hour clock in every supported language',()=>{for(const language of ['ru','en','es']){const value=stamp('2026-10-20T12:45:00Z',language);assert.match(value,/2026/);assert.match(value,/14:45:00/);}assert.match(stamp('2026-10-20T12:45:00Z','en'),/20\/10\/2026/);});
