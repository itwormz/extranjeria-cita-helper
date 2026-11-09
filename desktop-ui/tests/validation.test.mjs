import {test} from 'node:test';import assert from 'node:assert/strict';import {profileErrors} from '../src/validation.ts';
test('profile validation uses localized actionable messages and deduplicates errors',()=>{const actual=profileErrors(['label is required','applicant.contact.phone must be 6-15 digits when access_mode=certificate','applicant.contact.phone: требуется телефон из 6–15 цифр','request.province is not present in catalog'],key=>`translated:${key}`);assert.deepEqual(actual,['translated:Указать название профиля','translated:Телефон: от 6 до 15 цифр','translated:Выбрать провинцию']);assert.deepEqual(profileErrors(['unknown field SECRET'],key=>key),['Проверить данные профиля']);});

import {validateProtection,defaultProtection} from '../src/validation.ts';
test('appointment protection accepts only positive integers and booleans, defaults match Python',()=>{
 assert.deepEqual(defaultProtection,{confirm_new_round:true,daily_target:5,adaptive_target:true,distribute_attempts:false,retry_after_office:true,minimum_pause_minutes:15,first_cod_pause_minutes:30,next_cod_pause_minutes:60});
 assert.deepEqual(validateProtection({...defaultProtection}),[]);
 for(const key of ['daily_target','minimum_pause_minutes','first_cod_pause_minutes','next_cod_pause_minutes'])for(const bad of [0,-1,1.5,'7',null,NaN,2147483648])assert.deepEqual(validateProtection({...defaultProtection,[key]:bad}),[key],`${key}=${bad}`);
 for(const key of ['confirm_new_round','adaptive_target','distribute_attempts','retry_after_office'])assert.deepEqual(validateProtection({...defaultProtection,[key]:'yes'}),[key]);
 assert.deepEqual(validateProtection({...defaultProtection,daily_target:2147483647}),[]);
});

import {protectionFields} from '../src/protection.ts';
test('protection fields follow the Python order; flags and minutes come from the right place',()=>{
 assert.deepEqual(protectionFields.map(field=>field.key),['confirm_new_round','daily_target','adaptive_target','retry_after_office','minimum_pause_minutes','first_cod_pause_minutes','next_cod_pause_minutes','adaptive_reload_on_429','distribute_attempts']);
 assert.deepEqual(protectionFields.filter(field=>field.source==='timings').map(field=>field.key),['adaptive_reload_on_429']);
 assert.deepEqual(protectionFields.filter(field=>field.unit==='мин').map(field=>field.key),['minimum_pause_minutes','first_cod_pause_minutes','next_cod_pause_minutes']);
 for(const field of protectionFields){assert.ok(field.title&&field.description.length>20,field.key);}
});
