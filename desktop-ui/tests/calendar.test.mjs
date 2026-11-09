import test from 'node:test';import assert from 'node:assert/strict';import {dateISO,dateText,maskDate,monthDays,moveMonth} from '../src/calendar.ts';
test('typed calendar dates are validated rather than normalized across months',()=>{
 assert.equal(dateISO('29.02.2028'),'2028-02-29');for(const value of ['29.02.2026','31.04.2026','1.2.2026','00.10.2026','12.00.2026','12.12.0000'])assert.equal(dateISO(value),null);
 assert.equal(dateText('2026-10-25'),'25.10.2026');assert.equal(maskDate('12'),'12.');assert.equal(maskDate('1210'),'12.10.');assert.equal(maskDate('12.10.2026abc'),'12.10.2026');
});
test('calendar uses a stable Monday grid and month navigation across year boundaries',()=>{
 const cells=monthDays('2026-10-01');assert.equal(cells.length,42);assert.equal(cells[0],'2026-09-28');assert.equal(cells[41],'2026-11-08');assert.equal(moveMonth('2026-12-01',1),'2027-01-01');assert.equal(moveMonth('2026-01-01',-1),'2025-12-01');
});
