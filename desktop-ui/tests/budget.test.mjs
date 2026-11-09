import test from 'node:test';import assert from 'node:assert/strict';
import {parseDailyTarget,budgetLine,sourceKey,waitReasonKey,attemptWindowLabel,showTryNowButton,canTryNow,canResume,historyLines,fmt} from '../src/budget.ts';
const identity=key=>key;
const summary=(extra={})=>({wait_reason:null,attempt_window:null,target_source:'base',manual_target:null,adaptive_enabled:true,adaptive_target:null,base_target:5,target:5,sent:0,remaining:5,disabled:false,banners:0,no_slots:0,immediate_attempt:false,cod_oper_count:0,ready:'2026-11-10T09:30:00Z',restricted:false,offices:0,next_target:null,episode_history:[],episode_offices:[],first_cod_oper:null,last_cod_oper:null,...extra});

test('daily target accepts only a plain integer from 1 to 2147483647',()=>{
 for(const [text,value] of [['1',1],['12',12],['2147483647',2147483647]])assert.equal(parseDailyTarget(text),value,text);
 for(const text of ['0','-1','1.5','1e3',' 7 ','2147483648','','abc','007','+5','٣','1,000','0x10','99999999999999999999'])assert.equal(parseDailyTarget(text),null,JSON.stringify(text));
});

test('budget line shows target and remainder, then distribution and restriction in Madrid time',()=>{
 assert.equal(budgetLine(summary({target:12,remaining:9}),identity),'Бюджет 12 · Остаток 9');
 assert.equal(budgetLine(summary({target:12,remaining:9,disabled:true,restricted:true}),identity),'Бюджет 12 · Остаток 9 · Без распределения\nВозможное ограничение · Проба с 10:30');
 assert.equal(budgetLine(summary({restricted:true,ready:'2026-07-10T09:30:00Z'}),identity),'Бюджет 5 · Остаток 5\nВозможное ограничение · Проба с 11:30','summer time is UTC+2');
 const spanish=key=>({'Бюджет {target} · Остаток {remaining}':'Presupuesto {target} · Restan {remaining}'})[key]??key;
 assert.equal(budgetLine(summary({target:3,remaining:1}),spanish),'Presupuesto 3 · Restan 1');
});

test('source, wait reason and the attempt window label follow Python',()=>{
 assert.deepEqual(['manual','adaptive','base'].map(source=>sourceKey(summary({target_source:source}))),['Задан вручную','Адаптивный бюджет','Базовый бюджет']);
 assert.deepEqual(['window','minimum_pause','cod_oper',null].map(waitReasonKey),['Ждём окно распределения','Пауза между попытками','Пауза после cod. oper',null]);
 assert.deepEqual(attemptWindowLabel(summary({wait_reason:'window'})),{caption:'До',text:'10:30',visible:true});
 assert.deepEqual(attemptWindowLabel(summary({attempt_window:['2026-11-10T08:30:00Z','2026-11-10T09:30:00Z']})),{caption:'Окно',text:'09:30–10:30',visible:true});
 assert.deepEqual(attemptWindowLabel(summary({attempt_window:['2026-11-10T08:30:00Z','2026-11-10T09:30:00Z'],restricted:true})),{caption:'Окно',text:'',visible:false},'a restriction hides the window');
 assert.deepEqual(attemptWindowLabel(summary({immediate_attempt:true})),{caption:'Окно',text:'Открыто сейчас',visible:true});
 assert.deepEqual(attemptWindowLabel(summary()),{caption:'Окно',text:'',visible:false});
 assert.deepEqual(attemptWindowLabel(null),{caption:'Окно',text:'',visible:false});
});

test('Try now and Continue searching are available only in the right states',()=>{
 const run={active:true,manual:false,paused:false};
 assert.equal(canTryNow(run),true);
 assert.equal(canTryNow({...run,manual:true}),false,'the person owns the browser');
 assert.equal(canTryNow({...run,active:false}),false);
 assert.equal(canTryNow(null),false);
 assert.equal(showTryNowButton(run,summary({wait_reason:'window'})),true);
 assert.equal(showTryNowButton(run,summary({wait_reason:'cod_oper'})),true);
 assert.equal(showTryNowButton(run,summary({wait_reason:'minimum_pause'})),false,'the minimum pause cannot be skipped');
 assert.equal(showTryNowButton(run,summary()),false);
 assert.equal(showTryNowButton({...run,manual:true},summary({wait_reason:'window'})),false);
 assert.equal(canResume({active:true,manual:true}),true);
 assert.equal(canResume({active:true,manual:false}),false);
 assert.equal(canResume({active:false,manual:true}),false,'a finished search has nothing to resume');
 assert.equal(canResume(undefined),false);
});

test('restriction history lists the current and finished episodes in Madrid time',()=>{
 const finished={first:'2026-11-09T18:00:00Z',last:'2026-11-09T18:20:00Z',recovered:'2026-11-09T18:40:00Z',offices:[{at:'2026-11-09T18:30:00Z',names:['Oficina A','Oficina B'],result:'cod_oper'},{at:'2026-11-09T18:35:00Z',names:['Oficina C'],result:null}]};
 const lines=historyLines(summary({episode_history:[finished],first_cod_oper:'2026-11-10T09:00:00Z',last_cod_oper:'2026-11-10T09:10:00Z',episode_offices:[{at:'2026-11-10T09:05:00Z',names:['Oficina D'],result:'booking'}]}),identity);
 assert.deepEqual(lines,[
  'Первый cod oper: 2026-11-10 10:00 · Последний: 2026-11-10 10:10','10:05 · Oficina D · Показан выбор даты','',
  'Первый cod oper: 2026-11-09 19:00 · Последний: 2026-11-09 19:20','Восстановление между 2026-11-09 19:20 и 2026-11-09 19:40',
  '19:30 · Oficina A, Oficina B · Сит нет · cod oper','19:35 · Oficina C · Исход неизвестен','']);
 assert.deepEqual(historyLines(summary(),identity),[],'no episodes means no lines');
});

test('fmt fills named placeholders and leaves unknown ones alone',()=>{
 assert.equal(fmt('Бюджет {target} · {missing}',{target:5}),'Бюджет 5 · {missing}');
});
