import test from 'node:test';import assert from 'node:assert/strict';
import {LatestRequest} from '../src/latest-request.ts';
test('a channel switch queues a fresh check and suppresses the old response',async()=>{
 const checks=new LatestRequest();let finish;const wait=new Promise(resolve=>finish=resolve);
 const old=checks.run(async()=>{await wait;return 'stable';});
 await Promise.resolve();checks.invalidate();
 const latest=checks.run(async()=> 'beta');finish();
 assert.equal(checks.isCurrent(await old),false);
 const result=await latest;assert.equal(checks.isCurrent(result),true);assert.equal(result.value,'beta');
});
