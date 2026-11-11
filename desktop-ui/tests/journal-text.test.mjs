import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
import {journalMessage,journalTemplates} from '../src/journal-text.ts';
const base={id:1,time:0,stage:'province',status:'processing',event:'visit',destination:null,offices:[],selected_office:'',submitted:false,checked:false,applicant_continue:false,deadline:null,booking_details:{}};
const row=extra=>({...base,...extra});
test('an unexpected page after a visit explains itself instead of repeating the status',()=>{
 const unexpected=journalMessage(row({status:'unconfirmed',destination:'procedure'}));assert.equal(unexpected.template,'Другая страница → {destination}');assert.equal(unexpected.values.destination,'procedure');
 assert.equal(journalMessage(row({stage:'province',status:'unconfirmed',destination:'province'})).template,'Страница обновилась');
 assert.equal(journalMessage(row({status:'unconfirmed',destination:'unknown'})).template,'Ответ не распознан → {destination}');
});
test('visits tell what the bot does, sent, or confirmed per page',()=>{
 assert.equal(journalMessage(row({stage:'province'})).template,'Выбираем провинцию');
 assert.equal(journalMessage(row({stage:'province',submitted:true})).template,'Выбор провинции отправлен — ожидается ответ');
 assert.equal(journalMessage(row({stage:'request_menu',submitted:true,checked:true})).template,'Запрос проверки сит отправлен — ожидается ответ');
 assert.equal(journalMessage(row({stage:'procedure',status:'passed',destination:'applicant'})).template,'→ {destination}');
 assert.equal(journalMessage(row({stage:'auth_wait',status:'passed',destination:'applicant'})).template,'Вход выполнен → {destination}');
 assert.equal(journalMessage(row({stage:'request_no_slots',status:'checked'})).template,'Сит нет');
 assert.equal(journalMessage(row({stage:'request_no_slots',status:'waiting',deadline:1})).template,'Сит нет');
 assert.equal(journalMessage(row({stage:'applicant_no_slots',status:'waiting',deadline:1})).template,'Ожидаем повторную попытку');
});
test('offices, manual states and control rows have their own texts',()=>{
 const found=journalMessage(row({stage:'office',status:'offices_found',offices:['A']}));assert.equal(found.template,'Доступен офис: {offices}');assert.equal(found.values.offices,'A');
 assert.equal(journalMessage(row({stage:'office',status:'choice_required',offices:['A','B']})).template,'Доступно несколько офисов: {offices} — требуется выбор');
 assert.equal(journalMessage(row({stage:'auth',status:'login_required'})).template,'Открыта авторизация — требуется выполнить вход');
 assert.equal(journalMessage(row({event:'start',stage:null,status:'processing'})).template,'Поиск запущен');
 assert.equal(journalMessage(row({event:'finish',stage:null,status:'passed'})).template,'Поиск завершён');
 assert.equal(journalMessage(row({event:'finish',status:'cancelled'})).template,'Работа прервана пользователем');
 assert.equal(journalMessage(row({event:'flow_restart',stage:null,status:'new_pass'})).template,'Начинаем новый проход');
});
test('every template has an English and a Spanish translation',()=>{
 const messages=JSON.parse(readFileSync(new URL('../src/messages.json',import.meta.url)));
 for(const template of journalTemplates)for(const language of ['en','es'])assert.ok(messages[language][template],`${language}: ${template}`);
});
