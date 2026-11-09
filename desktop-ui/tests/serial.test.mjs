import {test} from 'node:test';
import assert from 'node:assert/strict';
import {SerialWrites} from '../src/serial.ts';
test('rapid selections/preferences execute in user order and failures do not block later writes',async()=>{const writes=new SerialWrites();const actual=[];let release;const blocked=new Promise(resolve=>release=resolve);const first=writes.enqueue(async()=>{await blocked;actual.push('first');throw new Error('synthetic');});const rejected=assert.rejects(first,/synthetic/);const second=writes.enqueue(async()=>{actual.push('second');});await Promise.resolve();assert.deepEqual(actual,[]);release();await rejected;await second;await writes.settled();assert.deepEqual(actual,['first','second']);});
