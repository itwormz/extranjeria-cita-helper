import test from 'node:test';import assert from 'node:assert/strict';
import {statisticGroups,metricValue} from '../src/statistics.ts';
import {readFileSync} from 'node:fs';
const icons=JSON.parse(readFileSync(new URL('../src/icons.json',import.meta.url),'utf8'));
const keys=group=>statisticGroups.find(g=>g.title===group).rows.map(r=>r[0]);

test('availability and error groups carry the three no-slots counters in Python order',()=>{
 assert.deepEqual(keys('Доступность'),['offices_seen','unique_offices','booking_screens','banners','no_slots']);
 assert.deepEqual(keys('Ошибки и восстановление'),['server_errors','rate_limited','blocked','flow_passes','cod_oper_count']);
 const labels=Object.fromEntries(statisticGroups.flatMap(g=>g.rows.map(r=>[r[0],r[1]])));
 assert.equal(labels.banners,'«Сит нет» до запроса');assert.equal(labels.no_slots,'Ответы «Сит нет» на запрос записи');assert.equal(labels.cod_oper_count,'Ответы «Сит нет» с cod. oper');
 for(const group of statisticGroups)for(const [key,,icon] of group.rows)assert.ok(icons[icon],`icon ${icon} for ${key}`);
});

test('unknown legacy counters show a dash, observed zeros stay zero, and a period filter changes the numbers',()=>{
 const all={banners:120,no_slots:33,cod_oper_count:2,legacy_unknown_fields:[]};
 const today={banners:3,no_slots:0,cod_oper_count:0,legacy_unknown_fields:[]};
 assert.deepEqual(['banners','no_slots','cod_oper_count'].map(k=>metricValue(all,k)),[120,33,2]);
 assert.deepEqual(['banners','no_slots','cod_oper_count'].map(k=>metricValue(today,k)),[3,0,0]);
 const legacy={banners:0,no_slots:0,cod_oper_count:0,legacy_unknown_fields:['banners','cod_oper_count']};
 assert.deepEqual(['banners','no_slots','cod_oper_count'].map(k=>metricValue(legacy,k)),[null,0,null]);
});
