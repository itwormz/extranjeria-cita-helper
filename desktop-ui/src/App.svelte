<script lang="ts">
import {onMount} from 'svelte';import {journalText,nextSelection} from './journal-copy';
import {searchSnapshot,startSearch,controlSearch,showSearchBrowser,copyText,copyStartupReport,resetSession,loadStatistics,loadBudget,setDailyTarget,tryAppointmentNow,resumeSearch,type SearchSnapshot,type SearchRow} from './bridge';
import {budgetLine,sourceKey,waitReasonKey,attemptWindowLabel,showTryNowButton,canResume,type BudgetSummary} from './budget';
import DailyBudgetDialog from './DailyBudgetDialog.svelte';
import {pages,statuses,searchErrors,countdown} from './work-state';
import {metricValue} from './statistics';
import {SerialWrites} from './serial';
import {PersistedValue} from './persisted-value';
import SettingsPreferences from './SettingsPreferences.svelte';
import StatisticsPanel from './StatisticsPanel.svelte';
import Icon from './Icon.svelte';import AboutDialog from './AboutDialog.svelte';
import {bootstrap,loadEditor,savePreferences,selectProfile,openDiagnostics,type Bootstrap,type ProfileEditorData,type Preferences} from './bridge';
import ProfileEditor from './ProfileEditor.svelte';
import {translate} from './i18n';
let data=$state<Bootstrap|null>(null);let native=$state(false);let error=$state('');let tab=$state('work');let journal=$state('timeline');let selected=$state<string|null>(null);let language=$state('ru');let dark=$state(true);let sound=$state(true);let diagnostics=$state(false);let automaticUpdates=$state(true);let editor=$state<ProfileEditorData|null>(null);let editingId=$state<string|null>(null);let loadingEditor=$state(false);let profileMenu=$state(false);let languageMenu=$state(false);let about=$state(false);
let search=$state<SearchSnapshot|null>(null);let searchBusy=$state(false);let now=$state(Date.now());let statsRefresh=$state(0);let allChecks=$state<number|null>(null);
let idleBudget=$state<BudgetSummary|null>(null);let budgetDialog=$state(false);let notice=$state('');let confirmRequest=$state(false);
let idleFor=$state<string|null>(null);
let budget=$derived(search?.active&&search.profile===selected?search.appointment_budget:idleFor===selected?idleBudget:null);
$effect(()=>{const id=selected;const active=search?.active;let current=true;if(native&&id&&!active)loadBudget(id).then(value=>{if(current){idleBudget=value;idleFor=id;}}).catch(()=>{if(current){idleBudget=null;idleFor=id;}});return()=>{current=false;};});
onMount(()=>{let disposed=false;let timer:ReturnType<typeof setTimeout>;let statsAt=0;async function poll(){try{const value=await searchSnapshot();if(disposed)return;search=value;now=Date.now();if(value.active&&value.profile)selected=value.profile;if(now-statsAt>5000){statsAt=now;statsRefresh++;const id=value.active?value.profile:selected;if(id){const summary=await loadStatistics(id);if(!disposed&&selected===id)allChecks=metricValue(summary?.all??{},'checks');}}}catch{/* Non-native HTML preview has no worker. */}if(!disposed)timer=setTimeout(poll,500);}void poll();return()=>{disposed=true;clearTimeout(timer);};});
$effect(()=>{const id=selected;const connected=native;let current=true;allChecks=null;if(connected&&id)loadStatistics(id).then(value=>{if(current)allChecks=metricValue(value?.all??{},'checks');}).catch(()=>{});return()=>{current=false;};});
async function runSearch(){if(!selected||searchBusy)return;searchBusy=true;try{await writes.settled();await startSearch(selected);error='';search=await searchSnapshot();}catch(e){error=searchErrors[String(e)]??'Не удалось запустить поиск';}finally{searchBusy=false;}}
async function searchCommand(command:'pause'|'resume'|'stop'){try{await controlSearch(command);search=await searchSnapshot();error='';}catch{error='Не удалось изменить состояние поиска';}}
async function commandFailed(e:unknown,fallback:string){const code=String(e);error=searchErrors[code]??fallback;}
async function tryNowCommand(){try{await tryAppointmentNow();error='';notice='Ближайшая попытка разрешена; остальные тайминги сохранены';}catch(e){await commandFailed(e,'Не удалось изменить состояние поиска');}}
async function resumeCommand(){try{await resumeSearch();error='';search=await searchSnapshot();}catch(e){await commandFailed(e,'Не удалось изменить состояние поиска');}}
async function saveBudget(target:number|null){const running=Boolean(search?.active&&search.profile===selected);await setDailyTarget(target,selected);budgetDialog=false;notice=running?'Бюджет применится со следующего действия':'';if(!running&&selected){idleBudget=await loadBudget(selected);idleFor=selected;}error='';}
let reportCopied=$state(false);
async function copyReport(id:string|null|undefined){if(!id)return;try{await copyStartupReport(id);reportCopied=true;error='';setTimeout(()=>{reportCopied=false;},4000);}catch{error='Не удалось скопировать журнал';}}
async function resetBrowserSession(){if(!selected||searchBusy)return;searchBusy=true;try{const done=await resetSession(selected);notice=done==='reset'?'Сессия сброшена. Можно запустить поиск.':'Новый проход начнётся с начальной страницы';error='';}catch(e){error=searchErrors[String(e)]??'Не удалось запустить поиск';}finally{searchBusy=false;}}
async function showBrowser(){try{await showSearchBrowser();error='';}catch{error='Не удалось показать браузер';}}
let currentPage=$derived(search?.page?pages[search.page]??'Неизвестная страница':'Браузер ещё не открыт');
let searchStatus=$derived(search?.error?'Поиск остановлен с ошибкой':search?.manual?'Ручное управление':search?.paused?'Пауза':search?.attention?'Требуется участие':search?.active?(search.deadline?'Ожидание':'В работе'):'Готов к запуску');
let nextAction=$derived(search?.manual?'Выбрать дату, заполнить CAPTCHA и подтвердить запись в браузере':search?.error?'Проверить браузер и запустить поиск заново':search?.paused?'Продолжить поиск':search?.attention?'Продолжить в браузере':search?.active?'Продолжение поиска автоматически':'Для начала — «Запустить»');
let nextShown=$derived(search?.active&&!search.manual&&!search.paused&&!search.error&&search.deadline?(waitReasonKey(budget?.wait_reason)??nextAction):nextAction);
let attempt=$derived(attemptWindowLabel(budget));
const navigation=[{id:'work',label:'Работа',icon:'activity'},{id:'statistics',label:'Статистика',icon:'statistics'},{id:'settings',label:'Настройки',icon:'settings'}];
let statisticsSelected=$state<string|null>(null);
function openTab(id:string){if(id==='statistics')statisticsSelected=selected;tab=id;}
let profile=$derived(data?.profiles.find(p=>p.id===selected));
const t=(key:string)=>translate(key,language);
let picked=$state<number[]>([]);let anchor=$state<number|null>(null);let journalMenu=$state<{x:number;y:number}|null>(null);
const journalHeaders=$derived(journal==='timeline'?[t('Время'),t('Страница'),t('Статус'),t('Что произошло')]:[t('Время'),t('Доступные офисы')]);
function rowTime(row:SearchRow){return new Date(row.time).toLocaleTimeString(language,{hour12:false});}
function rowWhat(row:SearchRow){return row.offices.join(' · ')||[row.booking_details.office,...(row.booking_details.offers??[]).map(v=>[v.date,v.time].filter(Boolean).join(' '))].filter(Boolean).join(' · ')||t(row.destination?`→ ${pages[row.destination]??row.destination}`:row.event==='start'?'Поиск запущен':row.event==='finish'?'Поиск остановлен':statuses[row.status]??row.status);}
const journalRows=$derived.by(()=>{const rows=[...(search?.rows??[])].reverse();return journal==='timeline'?rows.map(row=>({id:row.id,cells:[rowTime(row),t(row.stage?pages[row.stage]??row.stage:'Управление'),t(statuses[row.status]??row.status),rowWhat(row)]})):rows.filter(r=>r.offices.length).map(row=>({id:row.id,cells:[rowTime(row),row.offices.join(' · ')]}));});
$effect(()=>{journal;picked=[];anchor=null;journalMenu=null;});
function pickRow(index:number,event:MouseEvent){picked=nextSelection(picked,index,anchor,{toggle:event.metaKey||event.ctrlKey,range:event.shiftKey});anchor=index;}
async function copyJournal(all=false){journalMenu=null;try{await copyText(journalText(journalHeaders,journalRows.map(r=>r.cells),all?[]:picked));notice='Журнал скопирован';error='';}catch{error='Не удалось скопировать журнал';}}
function journalKey(event:KeyboardEvent){if((event.metaKey||event.ctrlKey)&&event.key.toLowerCase()==='c'&&picked.length){event.preventDefault();void copyJournal();}else if(event.key==='Escape'){picked=[];journalMenu=null;}}
const writes=new SerialWrites();
let preferenceWrites:PersistedValue<Preferences>|null=null;let selectionWrites:PersistedValue<string|null>|null=null;
let preferences=$derived({dark,sound,diagnostics,automatic_updates:automaticUpdates,language});
function initializeWrites(next:Bootstrap){if(!preferenceWrites){applyPreferences(next.preferences);preferenceWrites=new PersistedValue({...next.preferences},savePreferences,writes);}if(!selectionWrites)selectionWrites=new PersistedValue<string|null>(next.selected_profile,async id=>{if(id)await selectProfile(id);},writes);}
function applyPreferences(value:Preferences){dark=value.dark;sound=value.sound;diagnostics=value.diagnostics;automaticUpdates=value.automatic_updates;language=value.language;if(data)data.preferences=value;}
async function persistPreferences(){if(!data||!preferenceWrites)return;const controller=preferenceWrites;const result=await controller.set({...preferences});if(controller.isCurrent(result)){applyPreferences(result.value);if(!result.ok)error='Не удалось сохранить настройки';else if(error==='Не удалось сохранить настройки')error='';}}
function changePreference(key:keyof Preferences,value:boolean|string){if(key==='dark')dark=Boolean(value);else if(key==='sound')sound=Boolean(value);else if(key==='diagnostics')diagnostics=Boolean(value);else if(key==='automatic_updates')automaticUpdates=Boolean(value);else if(key==='language')language=String(value);void persistPreferences();}
async function chooseProfile(id:string){if(!selectionWrites)return;selected=id;profileMenu=false;const controller=selectionWrites;const result=await controller.set(id);if(controller.isCurrent(result)){selected=result.value;if(!result.ok)error='Не удалось сохранить выбор профиля';else if(error==='Не удалось сохранить выбор профиля')error='';}}
async function openFolder(){try{await openDiagnostics();}catch{error='Не удалось открыть папку диагностики';}}

const service=(procedure:string)=>t(procedure==='regreso'?'Autorización de regreso':'Toma de huellas / TIE');
onMount(()=>{bootstrap().then(result=>{data=result.data;native=result.native;selected=data.selected_profile;language=data.preferences.language;dark=data.preferences.dark;sound=data.preferences.sound;diagnostics=data.preferences.diagnostics;automaticUpdates=data.preferences.automatic_updates;initializeWrites(result.data);}).catch(()=>{error='Не удалось прочитать профили';});});
function closePopups(event:PointerEvent){const target=event.target as Element;if(!target.closest('.profile-picker'))profileMenu=false;if(!target.closest('.language-picker'))languageMenu=false;}
async function editProfile(id:string|null){if(loadingEditor)return;loadingEditor=true;try{await writes.settled();editor=await loadEditor(id);editingId=id;error='';}catch{error='Не удалось открыть профиль';}finally{loadingEditor=false;}}
async function profileSaved(id:string){editor=null;selected=id;try{if(!selectionWrites){const initial=await bootstrap();data=initial.data;initializeWrites(initial.data);}const selection=await selectionWrites?.set(id);if(selection&&!selection.ok)throw new Error('selection_failed');const result=await bootstrap();data=result.data;error='';}catch{error='Профиль сохранён. Не удалось обновить список профилей';}}
function keyboard(event:KeyboardEvent){if(event.key==='Escape'){profileMenu=false;languageMenu=false;about=false;}}
</script>
<svelte:window onpointerdown={closePopups} onkeydown={keyboard}/>
<div class:light={!dark} class="app-shell">
<header>
 <nav aria-label={t('Основная навигация')}>
 {#each navigation as item}<button class:active={tab===item.id} onclick={()=>openTab(item.id)}><Icon name={item.icon}/><span>{t(item.label)}</span></button>{/each}
 </nav>
 <div class="utilities">
  <div class="language-picker"><button class="icon-button language" title={t('Язык')} aria-label={t('Язык')} aria-expanded={languageMenu} onclick={()=>languageMenu=!languageMenu}>{language.toUpperCase()}</button>{#if languageMenu}<div class="popup language-popup">{#each ['ru','en','es'] as code}<button class:chosen={language===code} onclick={()=>{language=code;languageMenu=false;persistPreferences();}}>{code.toUpperCase()}</button>{/each}</div>{/if}</div>
  <button class="icon-button" title={t('Тёмная тема')} aria-label={t('Тёмная тема')} aria-pressed={dark} onclick={()=>{dark=!dark;persistPreferences();}}><Icon name={dark?'sun':'moon'}/></button>
  <button class="icon-button" title={t('Звук')} aria-label={t('Звук')} aria-pressed={sound} onclick={()=>{sound=!sound;persistPreferences();}}><Icon name={sound?'bell':'bell_off'}/></button>
  <button class="icon-button" title={t('О приложении')} aria-label={t('О приложении')} onclick={()=>about=true}><Icon name="info"/></button>
 </div>
</header>
<main>
 {#if error}<p class="error" role="alert">{t(error)}</p>{/if}
 {#if tab==='work'}
 <div class="profile-row"><label for="profile-trigger">{t('Профиль')}</label><div class="profile-picker"><button id="profile-trigger" disabled={Boolean(search?.active)} class="profile-trigger" aria-expanded={profileMenu} aria-haspopup="listbox" onclick={()=>profileMenu=!profileMenu}><span>{profile?.label??t('Выбрать профиль')}</span><Icon name="chevron-down" size={16}/></button>{#if profileMenu}<div class="popup profile-options" role="listbox" aria-label={t('Профиль')} tabindex="-1">{#each data?.profiles??[] as option}<button role="option" aria-selected={selected===option.id} onclick={()=>chooseProfile(option.id)}>{option.label}</button>{:else}<span class="empty-option">{t('Профилей пока нет')}</span>{/each}</div>{/if}</div></div>
 <div class="service"><Icon name="id" size={16}/><span>{profile?service(profile.procedure):'—'}</span></div>
 <section class="status" aria-label={t('Состояние поиска')}>
  <div class="status-lines"><span class="caption">{t('Страница')}</span><strong>{t(currentPage)}</strong><span class="caption">{t('Бот')}</span><strong class:muted={!search?.active} class:attention={search?.attention}>{t(searchStatus)}</strong><span class="caption">{t('Далее')}</span><strong>{t(nextShown)}</strong></div>
  <div class="timer"><b>{countdown(search?.deadline??null,now)}</b><span>{t('до следующего действия')}</span></div>
 </section>
 <p class="participation muted" class:attention={search?.attention}>{t(search?.attention||search?.manual?'Требуется участие в браузере':'Участие не требуется')}</p>{#if search?.error}<p class="error" role="alert">{t(searchErrors[search.error]??'Поиск остановлен с ошибкой')}</p>{/if}{#if search?.error&&search.report_id}<div class="report-band"><Icon name="alert" size={18}/><span class="muted">{t('Скопировать отчёт и отправить автору для проверки.')} {t('Без данных профиля и содержимого страниц')} · {t('Отчёт')} CEH-{search.report_id}</span><button type="button" onclick={()=>void copyReport(search?.report_id)}><Icon name={reportCopied?'check':'copy'} size={16}/>{t(reportCopied?'Отчёт скопирован':'Скопировать отчёт')}</button></div>{/if}{#if search?.active&&search.warning}<p class="muted" role="status">{t(searchErrors[search.warning]??search.warning)}</p>{/if}
 <div class="controls">{#if canResume(search)}<button class="primary" onclick={()=>void resumeCommand()}><Icon name="play"/>{t('Продолжить поиск')}</button>{:else if search?.active&&!search.manual}<button class="primary" onclick={()=>searchCommand(search?.paused?'resume':'pause')}><Icon name={search.paused?'play':'pause'}/>{t(search.paused?'Продолжить':'Пауза')}</button>{:else}<button class="primary" disabled={!native||!profile?.valid||searchBusy||Boolean(search?.active)} onclick={()=>void runSearch()}><Icon name="play"/>{t('Запустить')}</button>{/if}{#if showTryNowButton(search,budget)}<button class="flat" onclick={()=>void tryNowCommand()}><Icon name="clock"/>{t('Попробовать сейчас')}</button>{/if}<button class="flat" disabled={!search?.browser_pid} onclick={()=>void showBrowser()}><Icon name="external"/>{t('Браузер')}</button><button class="flat" disabled={!selected||searchBusy} onclick={()=>void resetBrowserSession()}><Icon name="restart"/>{t(search?.active?'Перезапустить сессию':'Сбросить сессию')}</button><button class="flat" disabled={!selected} onclick={()=>{confirmRequest=true;openTab('statistics');}}><Icon name="check"/>{t('Подтвердить запись вручную')}</button>{#if search?.active}<button class="flat" onclick={()=>void searchCommand('stop')}><Icon name="stop"/>{t('Остановить')}</button>{/if}</div>
 <p class="metrics muted">{t('Проверки')}: {search?.counters.checks??0} {t('за запуск')} · {t('Всего')}: {allChecks??'—'}</p>
 {#if budget}<div class="budget-row"><p class="muted budget-line">{#each budgetLine(budget,t).split('\n') as part}<span>{part}</span>{/each}</p><button class="icon-button" title={`${t('Изменить на сегодня')}\n${t(sourceKey(budget))}`} aria-label={t('Изменить на сегодня')} onclick={()=>budgetDialog=true}><Icon name="pencil" size={16}/></button></div>{/if}
 {#if attempt.visible}<p class="muted attempt-window"><span>{t(attempt.caption)}</span> <strong title={t('Окно попытки по Мадриду')}>{t(attempt.text)}</strong></p>{/if}
 {#if notice}<p class="muted" role="status">{t(notice)}</p>{/if}
 <section class="journals">
  <div class="journal-tabs"><div role="tablist" aria-label={t('Журнал')}><button role="tab" aria-selected={journal==='timeline'} class:active={journal==='timeline'} onclick={()=>journal='timeline'}>{t('Таймлайн')}</button><button role="tab" aria-selected={journal==='offices'} class:active={journal==='offices'} onclick={()=>journal='offices'}>{t('Доступные офисы')}</button></div><button class="icon-button" disabled={!journalRows.length} onclick={()=>void copyJournal()} aria-label={t(picked.length?'Копировать выделенное':'Копировать журнал')} title={t(picked.length?'Копировать выделенное':'Копировать журнал')+(picked.length?' (⌘C / Ctrl+C)':'')}><Icon name="clipboard" size={18}/></button></div>
  <div class="journal-content">{#if journalRows.length}<table class="work-log" role="grid" aria-label={t('Журнал')} tabindex="0" onkeydown={journalKey} oncontextmenu={e=>{e.preventDefault();journalMenu={x:e.clientX,y:e.clientY};}}><thead><tr>{#each journalHeaders as head}<th>{head}</th>{/each}</tr></thead><tbody>{#each journalRows as row,index (row.id)}<tr class:picked={picked.includes(index)} onclick={e=>pickRow(index,e)}>{#each row.cells as cell}<td>{cell}</td>{/each}</tr>{/each}</tbody></table>{#if journalMenu}<div class="journal-menu" role="menu" style={`left:${journalMenu.x}px;top:${journalMenu.y}px`}><button role="menuitem" disabled={!picked.length} onclick={()=>void copyJournal()}>{t('Копировать выделенное')}</button><button role="menuitem" onclick={()=>void copyJournal(true)}>{t('Копировать весь журнал')}</button></div>{/if}{:else}<div class="empty-state"><Icon name={journal==='timeline'?'list':'office'} size={28}/><p>{t(journal==='timeline'?'Записей пока нет':'Офисы пока не появлялись')}</p></div>{/if}</div>
 </section>
 {:else if tab==='settings'}
 <div class="page-heading"><h1><Icon name="settings" size={24}/>{t('Настройки')}</h1></div>
 <section class="settings-profiles"><div class="profiles-heading"><h2><Icon name="user"/>{t('Профили')}</h2><button class="icon-button" title={t('Добавить профиль')} aria-label={t('Добавить профиль')} disabled={loadingEditor} onclick={()=>editProfile(null)}><Icon name="plus"/></button></div>{#if data?.profiles.length}<div class="profile-list">{#each data.profiles as item}<button class:chosen={selected===item.id} onclick={()=>editProfile(item.id)}><Icon name="user"/><span><strong>{item.label}</strong><small>{service(item.procedure)} · {item.province}</small></span><Icon name="pencil" size={16}/></button>{/each}</div>{:else}<div class="settings-empty"><Icon name="user" size={28}/><p>{t('Профилей пока нет')}</p></div>{/if}</section>
 <SettingsPreferences {preferences} {t} onchange={changePreference} onfolder={()=>void openFolder()}/>
 {:else}
 <StatisticsPanel live={search?.active&&search.profile===(statisticsSelected??selected)?{active:true,manual:search.manual,budget:search.appointment_budget}:null} {confirmRequest} onconfirmed={()=>confirmRequest=false} revision={statsRefresh} profiles={data?.profiles??[]} selected={statisticsSelected??selected} onselect={id=>statisticsSelected=id} {t} {language} {dark}/>
 {/if}
 <footer><span>{t('Данные хранятся локально')} · {data?.version??'0.0.0-dev.1'}</span><span class="preview-label">Rust Preview{data?.preview_source==='legacy_copy'?` · ${t('Копия старых данных')}`:''}{native?'':` · ${t('Превью интерфейса')}`}</span></footer>
</main>
{#if editor}<ProfileEditor model={editor} {editingId} {t} {language} onclose={()=>editor=null} onsaved={id=>{void profileSaved(id);}}/>{/if}
{#if budgetDialog&&budget}<DailyBudgetDialog {budget} {t} onclose={()=>budgetDialog=false} onsave={saveBudget}/>{/if}
{#if about}<AboutDialog version={data?.version??'0.0.0-dev.1'} {language} {t} onclose={()=>about=false}/>{/if}
</div>

<style>.budget-row{display:flex;align-items:center;gap:8px}.budget-line{margin:0;display:flex;flex-direction:column}.attempt-window{margin:4px 0}.attention{color:var(--warning,#e9b44c)}.work-log{table-layout:fixed;border-collapse:collapse;width:100%;text-align:left;min-width:760px;font-size:13px}.work-log th{color:var(--muted);font-weight:400;position:sticky;top:0;background:var(--surface);z-index:1}.work-log th,.work-log td{padding:11px 12px;border-bottom:1px solid var(--line);vertical-align:top;overflow-wrap:anywhere}.work-log th:first-child,.work-log td:first-child{width:90px;white-space:nowrap;font-variant-numeric:tabular-nums}.work-log th:nth-child(2){width:190px}.work-log th:nth-child(3){width:160px}.journal-content{overflow:auto;max-height:55vh;min-height:160px}.timer b{font-variant-numeric:tabular-nums}</style>
