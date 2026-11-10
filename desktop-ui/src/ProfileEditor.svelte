<script lang="ts">
import {onMount,untrack} from 'svelte';
import {loadNotificationEditor,saveProfile,loadBudget,setDailyTarget,type ProfileEditorData} from './bridge';
import {protectionFields,searchRisk,type ProtectionField} from './protection';import {validateProtection,type Protection} from './validation';import {parseDailyTarget,budgetLine,sourceKey,type BudgetSummary} from './budget';import DailyBudgetDialog from './DailyBudgetDialog.svelte';
import {timingInputs,parseTimingInputs,timingGroups,timingTitles,type Timings} from './timings';
import NotificationPanel from './NotificationPanel.svelte';
import {notificationError,type NotificationDraft} from './notifications';
import {profileErrors} from './validation';
import Icon from './Icon.svelte';import Field from './Field.svelte';import Choice from './Choice.svelte';
let {model,editingId,t,language,onclose,onsaved}: {language:string;model:ProfileEditorData;editingId:string|null;t:(key:string)=>string;onclose:()=>void;onsaved:(id:string)=>void}=$props();
// Each editor owns a draft; cancelling never mutates the loaded profile.
let draft=$state(untrack(()=>({...structuredClone($state.snapshot(model.draft)),browser:model.draft.browser??'firefox'})));let saving=$state(false);let errors=$state<string[]>([]);let dialog:HTMLDialogElement;
let notifications=$state<NotificationDraft|null>(null);let notificationLoading=$state(false);let notificationsDirty=$state(false);let notificationFailure=$state('');let alive=true;
async function notificationTab(){tab='notifications';if(notifications||notificationLoading)return;notificationLoading=true;notificationFailure='';try{const result=await loadNotificationEditor(editingId,model.revision);if(alive)notifications=result.draft;}catch(reason){if(alive)notificationFailure=notificationError(reason,t);}finally{if(alive)notificationLoading=false;}}
let protection=$state<Protection>(untrack(()=>({...model.protection})));let protectionText=$state<Record<string,string>>(untrack(()=>Object.fromEntries(Object.entries(model.protection).filter(([,v])=>typeof v==='number').map(([k,v])=>[k,String(v)]))));let protectionDirty=$state(false);let budgetNote=$state<BudgetSummary|null>(null);let budgetDialog=$state(false);
let tab=$state('data');let timingsDirty=$state(false);let timingFields=$state(untrack(()=>timingInputs(model.timings,{decimals:2,comma:language!=='en'})));
const flagOn=(field:ProtectionField):boolean=>field.source==='timings'?timingFields[field.key]?.[0]!=='false':Boolean((protection as unknown as Record<string,unknown>)[field.key]);
function setFlag(field:ProtectionField,on:boolean){if(field.source==='timings'){timingFields[field.key]=[String(on)];timingsDirty=true;}else{(protection as unknown as Record<string,unknown>)[field.key]=on;protectionDirty=true;}}
const riskMessages=['Пока сайт сообщает, что сит нет, данные не отправляются повторно. Для следующей проверки используется штатная кнопка Salir.','Повторная отправка данных может повысить риск блокировки','Часть настроек защиты отключена. Сайт может временно ограничить отправку запросов.'];
const risk=$derived(searchRisk(draft.applicant_no_slots_mode==='continue',Object.fromEntries(protectionFields.filter(field=>field.kind==='flag').map(field=>[field.key,flagOn(field)]))));
let access=$state(draft.access_mode==='certificate'?'certificate':draft.access_mode==='clave'?draft.clave_method:'unconfigured');
let lastDocument=draft.document_type;let regresoDocument=draft.document_type;const documentDrafts=new Map([[draft.document_type,draft.document_number]]);
function documentChanged(type:string){documentDrafts.set(lastDocument,draft.document_number);draft.document_number=documentDrafts.get(type)??'';draft.document_type=type;lastDocument=type;}
function procedureChanged(service:string){if(service==='huellas'){regresoDocument=draft.document_type;documentChanged('NIE');}else documentChanged(regresoDocument);}
const choices=(values:string[])=>values.map(value=>({value,label:value}));
onMount(()=>{if(editingId)loadBudget(editingId).then(value=>{if(alive)budgetNote=value;}).catch(()=>{});dialog.showModal();dialog.querySelector<HTMLInputElement>('input')?.focus();return()=>{alive=false;notifications=null;dialog.close();};});
async function save(){if(saving||notificationLoading)return;if(!['certificate','movil','permanente'].includes(access)){errors=[t('Выбрать способ авторизации')];return;}let timings:Timings|null=null;if(timingsDirty){try{timings=parseTimingInputs($state.snapshot(timingFields),model.timings);}catch{errors=[t('Проверить значения интервалов')];tab='timings';return;}}
 const candidate:Record<string,unknown>={...$state.snapshot(protection)};for(const field of protectionFields)if(field.kind==='int'&&field.source==='protection')candidate[field.key]=parseDailyTarget(protectionText[field.key]??'')??NaN;
 if(protectionDirty&&validateProtection(candidate).length){errors=[t('Проверить значения защиты от бана')];return;}
 saving=true;errors=[];draft.access_mode=access==='certificate'?'certificate':'clave';if(access!=='certificate')draft.clave_method=access;
 try{const result=await saveProfile(editingId,draft,model.revision,timings,notificationsDirty&&notifications?$state.snapshot(notifications):null,protectionDirty?candidate as unknown as Protection:null);onsaved(result.id);}catch(error){const failure=error as {code?:string;errors?:string[]};errors=failure?.code?.startsWith('secrets_')?[notificationError(failure,t)]:failure?.code==='validation'?profileErrors(failure.errors??['unknown'],t):[t(failure?.code==='conflict'?'Файл изменён извне. Перед сохранением открыть настройки заново.':'Не удалось сохранить профиль')];}finally{saving=false;}}
</script>
<dialog class="profile-editor" bind:this={dialog} oncancel={event=>{event.preventDefault();if(!saving)onclose();}}>
<form novalidate onsubmit={event=>{event.preventDefault();void save();}}>
 <h1>{t(editingId?'Редактирование профиля':'Новый профиль')}</h1>
 <div class="editor-tabs" role="tablist" aria-label={t('Настройки профиля')}><button type="button" role="tab" aria-selected={tab==='data'} class:selected={tab==='data'} onclick={()=>tab='data'}>{t('Данные')}</button><button type="button" role="tab" aria-selected={tab==='timings'} class:selected={tab==='timings'} onclick={()=>tab='timings'}>{t('Тайминги и повторы')}</button><button type="button" role="tab" aria-selected={tab==='notifications'} class:selected={tab==='notifications'} onclick={()=>void notificationTab()}>{t('Уведомления')}</button></div>
 <div class="editor-scroll"><fieldset disabled={saving}>
 {#if tab==='data'}<div class="editor-grid">
  <Field label={t('Название профиля')} icon="user" bind:value={draft.label}/>
  <Choice label={t('Провинция')} icon="pin" bind:value={draft.province} options={choices(model.provinces)}/>
  <Choice label={t('Услуга')} icon="file_text" bind:value={draft.procedure} options={[{value:'huellas',label:'Toma de huellas / TIE'},{value:'regreso',label:'Autorización de regreso'}]} onchange={procedureChanged}/>
  <Choice label={t('Способ авторизации')} icon="key" bind:value={access} options={[{value:'certificate',label:t('Электронный сертификат')},{value:'movil',label:'Cl@ve Móvil'},{value:'permanente',label:'Cl@ve Permanente'}]}/>
  <div class="wide"><Choice label={t('Браузер')} icon="globe" bind:value={draft.browser} options={[{value:'firefox',label:t('Firefox — установленный')}]} /></div>
  <div class="identity-modes"><div class="mode-buttons" role="group" aria-label={t('NIE и имя')}><button type="button" class:selected={draft.fill_mode==='copy'} aria-pressed={draft.fill_mode==='copy'} onclick={()=>draft.fill_mode='copy'}><Icon name="copy" size={17}/>{t('Из авторизации')}</button><button type="button" class:selected={draft.fill_mode==='manual'} aria-pressed={draft.fill_mode==='manual'} onclick={()=>draft.fill_mode='manual'}><Icon name="pencil" size={17}/>{t('Ввести вручную')}</button></div>{#if draft.fill_mode==='copy'}<small class="muted">{t('NIE и имя будут взяты после входа на сайт.')}</small>{/if}</div>
  {#if draft.fill_mode!=='copy'}
   <div class="wide document-row">{#if draft.procedure==='regreso'}<div class="document-type"><Choice label={t('Документ')} icon="id" bind:value={draft.document_type} options={[{value:'NIE',label:'NIE'},{value:'DNI',label:'DNI'},{value:'PASSPORT',label:t('Паспорт')}]} onchange={documentChanged}/></div>{/if}
   <div class="document-number"><Field label={draft.document_type==='PASSPORT'?t('Паспорт'):draft.document_type} icon="id" bind:value={draft.document_number} maxlength={20}/></div></div>
   <div class="wide"><Field label={t('Имя и фамилия')} icon="user" bind:value={draft.full_name}/></div>
  {/if}
  <Field label={t('Телефон')} icon="phone" bind:value={draft.phone} type="tel" maxlength={24}/><Field label={t('Почта')} icon="mail" bind:value={draft.email} type="email" maxlength={254}/>
  {#if draft.procedure==='huellas'}<div class="wide"><Choice label={t('Гражданство')} icon="globe" bind:value={draft.nationality} options={choices(model.nationalities)}/></div>{/if}
 </div>{:else if tab==='timings'}
 <div class="timings-grid">
 <h2>{t('Проверка сит')}</h2>
 <Choice label={t('На форме заявителя')} icon="user" bind:value={draft.applicant_no_slots_mode} options={[{value:'wait',label:t('Ждать при «нет сит»')},{value:'continue',label:t('Вводить и продолжать')}]}/>
 <Choice label={t('После ответа «нет сит»')} icon="restart" bind:value={draft.no_slots_mode} options={[{value:'reload',label:t('Начать новый круг')},{value:'salir',label:t('Через Salir')}]}/>
 <small class="muted">{t(draft.applicant_no_slots_mode==='continue'?'Вводить или копировать данные даже при предупреждении «нет сит».':'Не вводить данные, пока форма сообщает об отсутствии сит.')}</small>
 <small class="muted">{t(draft.no_slots_mode==='reload'?'После паузы открыть начальную страницу и пройти поиск заново.':'После паузы нажать Salir и продолжить по странице сайта.')}</small>
 <h2>{t('Защита от бана')}</h2>
 {#each protectionFields as field}{#if field.kind==='flag'}<div class="protection-card" title={t(field.description)}><span>{t(field.title)}</span><i class="help"><Icon name="info" size={13}/></i><button type="button" class="switch" role="switch" aria-label={t(field.title)} aria-checked={flagOn(field)} onclick={()=>setFlag(field,!flagOn(field))}><span></span></button></div>{:else}<div class="timing-field" title={t(field.description)}><span>{t(field.title)}</span><i class="help"><Icon name="info" size={13}/></i><div class="timing-values"><input aria-label={t(field.title)} inputmode="numeric" maxlength="10" value={protectionText[field.key]} aria-invalid={parseDailyTarget(protectionText[field.key]??'')===null} oninput={event=>{protectionText[field.key]=event.currentTarget.value;protectionDirty=true;}}/>{#if field.unit}<small>{t(field.unit)}</small>{/if}</div></div>{/if}{/each}
 {#if budgetNote}<div class="budget-note"><span class="muted">{budgetLine(budgetNote,t).split('\n')[0]} · {t(sourceKey(budgetNote))}</span><button type="button" class="icon-button" title={t('Изменить на сегодня')} aria-label={t('Изменить на сегодня')} onclick={()=>budgetDialog=true}><Icon name="pencil" size={16}/></button></div>{/if}
 <div class="search-behavior" class:risk={risk.risk}><Icon name={risk.risk?'shield':'check'} size={18}/><div><strong>{t(risk.risk?'Возможны блокировки':'Рекомендуется для обычного поиска')}</strong><p>{t(riskMessages[risk.message])}</p></div></div>
 <small class="muted wide">{t('Выбор поведения не меняет интервалы и паузы восстановления.')}</small>
 {#each timingGroups as group}<h2>{t(group.title)}</h2>{#each group.fields as [key,label]}
 <div class="timing-field with-icon" title={t(timingTitles[key]??label)}><i class="lead"><Icon name={key==='alert_seconds'?'bell':key==='reload_seconds'?'globe':'info'} size={17}/></i><span>{t(label)}</span><i class="help"><Icon name="info" size={13}/></i><div class="timing-values">{#each timingFields[key] as _,index}{#if index>0}<span class="muted">—</span>{/if}<input aria-label={t(label)+(timingFields[key].length===2?(index===0?' · min':' · max'):'')} inputmode="decimal" autocomplete="off" spellcheck="false" bind:value={timingFields[key][index]} oninput={()=>timingsDirty=true}/>{/each}<small>{key.endsWith('_ms')?'ms':'s'}</small></div></div>
 {/each}{/each}
 </div>{:else}{#if notificationLoading}<p role="status" class="muted">{t('Загрузка…')}</p>{:else if notifications}<NotificationPanel bind:draft={notifications} {t} {language} profile={draft.label} province={draft.province} onchange={()=>notificationsDirty=true}/>{:else}<p class="notification-error" role="alert">{notificationFailure}</p><button type="button" onclick={()=>void notificationTab()}>{t('Повторить')}</button>{/if}{/if}</fieldset></div>
 <p class="editor-note muted">{t('Пароли и подтверждения — только в браузере.')}</p>
 {#if errors.length}<div class="editor-errors" role="alert">{#each errors as error}<p>{t(error)}</p>{/each}</div>{/if}
 <div class="editor-actions"><button class="primary" type="submit" disabled={saving||notificationLoading}><Icon name="check"/>{t('Сохранить профиль')}</button><button type="button" disabled={saving} onclick={onclose}><Icon name="close"/>{t('Отмена')}</button></div>
</form>
{#if budgetDialog&&budgetNote}<DailyBudgetDialog budget={budgetNote} {t} onclose={()=>budgetDialog=false} onsave={async target=>{await setDailyTarget(target,editingId);budgetNote=await loadBudget(editingId!);budgetDialog=false;}}/>{/if}
</dialog>
