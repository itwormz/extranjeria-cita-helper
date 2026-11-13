import test from 'node:test';import assert from 'node:assert/strict';
import {pairingLinkHint} from '../src/notifications.ts';
test('pairing hint uses the ticket lifetime, with no hardcoded two or five minute promise',()=>{
 const t=key=>key;
 assert.equal(pairingLinkHint(300,t),'Для другого получателя — отправить ему ссылку. Срок действия: 5 мин.');
 assert.equal(pairingLinkHint(120,t),'Для другого получателя — отправить ему ссылку. Срок действия: 2 мин.');
 assert.equal(pairingLinkHint(undefined,t),'Для другого получателя — отправить ему ссылку.');
});
