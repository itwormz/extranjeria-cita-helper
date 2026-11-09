<script lang="ts">
import {tick,untrack} from 'svelte';import Icon from './Icon.svelte';
import {metricValue,formatDuration,firstEvent,metricHint} from './statistics';
import Choice from './Choice.svelte';
import StatisticsHistory from './StatisticsHistory.svelte';
import AppointmentDialog from './AppointmentDialog.svelte';import AchievementPanel from './AchievementPanel.svelte';
import {loadStatistics,loadAppointmentContext,type Achievement,type AppointmentContext,type ProfileSummary,type StatisticsDashboard} from './bridge';
let {profiles,selected,onselect,t,language,dark,revision=0}:{profiles:ProfileSummary[];selected:string|null;onselect:(id:string)=>void;t:(key:string)=>string;language:string;dark:boolean;revision?:number}=$props();
let data=$state<StatisticsDashboard|null>(null);let loading=$state(false);let failed=$state(false);let period=$state('all');
let heading:HTMLDivElement;let refresh=$state(0);let context=$state<AppointmentContext|null>(null);let item=$state<Achievement|null>(null);let showForm=$state(false);let editing=$state(false);let actionBusy=$state(false);let actionError=$state(false);let view=$state('stats');
type BudgetEpisode = {first:string;last:string;recovered:string;offices:{at:string;names:string[];result:string|null}[]};
type BudgetSummary = {sent:number;target:number;remaining:number;banners:number;no_slots:number;ready:string;restricted:boolean;episode_history:BudgetEpisode[]};
let budget=$derived(data?.today.appointment_budget as BudgetSummary|undefined);
function budgetTime(value:string){return new Intl.DateTimeFormat(language,{timeZone:'Europe/Madrid',dateStyle:'short',timeStyle:'short'}).format(new Date(value));}
let values=$derived(data?.[period==='today'?'today':'all']??{});
$effect(()=>{const id=selected;refresh;let current=true;data=null;failed=false;loading=Boolean(id);if(id)loadStatistics(id).then(value=>{if(current)data=value;}).catch(()=>{if(current)failed=true;}).finally(()=>{if(current)loading=false;});return()=>{current=false;};});
$effect(()=>{revision;const id=selected;let current=true;if(id)loadStatistics(id).then(value=>{if(current){data=value;failed=false;}}).catch(()=>{if(current)failed=true;});return()=>{current=false;};});
$effect(()=>{const id=selected;let current=true;const keepAchievement=untrack(()=>view==='achievement');showForm=false;context=null;item=null;actionError=false;if(keepAchievement&&id){actionBusy=true;loadAppointmentContext(id).then(value=>{if(current){context=value;item=value.records.at(-1)??null;}}).catch(()=>{if(current)actionError=true;}).finally(()=>{if(current)actionBusy=false;});}return()=>{current=false;};});
async function openAppointment(edit=false){if(!selected||actionBusy)return;const id=selected;actionBusy=true;actionError=false;try{const value=await loadAppointmentContext(id);if(selected!==id)return;context=value;editing=edit;showForm=true;}catch{if(selected===id)actionError=true;}finally{actionBusy=false;}}
async function showAchievement(){if(!selected||actionBusy)return;const id=selected;actionBusy=true;actionError=false;try{const value=await loadAppointmentContext(id);if(selected!==id)return;context=value;item=value.records.at(-1)??null;view='achievement';}catch{if(selected===id)actionError=true;}finally{actionBusy=false;}}
function saved(value:Achievement){item=value;showForm=false;view='achievement';refresh++;}
$effect(()=>{view;void tick().then(()=>heading?.scrollIntoView({block:'start'}));});
const overview=[['checks','Проверок сит','search'],['active_seconds','Активный поиск','clock'],['days','Дни с поиском','calendar_days']];
const groups=[{title:'Доступность',icon:'eye',rows:[['offices_seen','Появлений офисов','office'],['unique_offices','Разных офисов','pin'],['booking_screens','Появления выбора даты','calendar_days'],['successes','Получено сит','calendar']]},{title:'Ошибки и восстановление',icon:'wrench',rows:[['server_errors','Ошибок сервера','server'],['rate_limited','Ограничений HTTP 429','clock'],['blocked','Ограничения доступа','shield'],['flow_passes','Повторные проходы','restart']]}];
function display(key:string){const value=metricValue(values,key);return key==='active_seconds'?formatDuration(value,language):value===null?'—':new Intl.NumberFormat(language).format(value);}

</script>
<section class="statistics-panel" class:showing-achievement={view==='achievement'}>
<div class="statistics-heading" bind:this={heading}><h1><Icon name="statistics" size={24}/>{t(view==='achievement'?'Достижения профиля':'Статистика профиля')}</h1><div class="statistics-profile"><Choice label={t('Профиль')} icon="user" value={selected??''} options={profiles.map(p=>({value:p.id,label:p.label}))} onchange={onselect}/></div></div>
{#if view==='stats'}
<div class="statistics-period" role="group" aria-label={t('Период')}>{#each [['all','За всё время'],['today','Сегодня']] as [id,label]}<button class:chosen={period===id} aria-pressed={period===id} onclick={()=>period=id}>{t(label)}</button>{/each}</div>
{#if !selected}<p class="muted">{t('Выбрать профиль')}</p>{:else if loading}<p class="muted" role="status">{t('Загрузка статистики…')}</p>{:else if failed}<p class="error" role="alert">{t('Не удалось прочитать статистику профиля')}</p>{:else if !data}<div class="settings-empty"><Icon name="statistics" size={28}/><p>{t('Истории поиска пока нет')}</p></div>{:else}
<div class="statistics-overview">{#each overview as [key,label,icon]}<div class="statistics-card"><strong title={metricHint(values,key,language,t)}>{display(key)}</strong><span><Icon name={icon}/>{t(label)}</span></div>{/each}</div>
<div class="statistics-groups">{#each groups as group}<section><h2><Icon name={group.icon}/>{t(group.title)}</h2>{#each group.rows as [key,label,icon]}<div class="statistics-row"><Icon name={icon}/><span>{t(label)}</span><strong title={metricHint(values,key,language,t)}>{display(key)}</strong></div>{/each}</section>{/each}</div>
<section class="statistics-effort"><h2><Icon name="settings"/>{t('Автоматизация')}</h2><div class="statistics-pairs">{#each [['actions_performed','Действий выполнено','pointer'],['pages_processed','Страниц обработано','file_text']] as [key,label,icon]}<div class="statistics-row"><Icon name={icon}/><span>{t(label)}</span><strong title={metricHint(values,key,language,t)}>{display(key)}</strong></div>{/each}</div></section>
{#if budget}
<section class="statistics-effort">
  <h2><Icon name="clock"/>{t('Попытки сегодня')}</h2>
  <div class="statistics-pairs">
    <div class="statistics-row"><span>{t('Отправлено / цель')}</span><strong>{budget.sent} / {budget.target}</strong></div>
    <div class="statistics-row"><span>{t('Следующая попытка')}</span><strong>{budgetTime(budget.ready)}</strong></div>
    <div class="statistics-row"><span>{t('Плашек на форме')}</span><strong>{budget.banners}</strong></div>
    <div class="statistics-row"><span>{t('Ответов без сит')}</span><strong>{budget.no_slots}</strong></div>
  </div>
  {#if budget.restricted}<p class="muted">{t('Возможное ограничение · Cod. Oper.')}</p>{/if}
  {#if budget.episode_history.length}
    <details><summary>{t('История возможных ограничений')}</summary>
      {#each [...budget.episode_history].reverse() as episode}
        <div class="statistics-row"><span>{budgetTime(episode.first)} — {budgetTime(episode.recovered)}</span><strong>{episode.offices.length}</strong></div>
        {#each episode.offices as office}<div class="muted">{budgetTime(office.at)} · {office.names.join(' · ')}</div>{/each}
      {/each}
    </details>
  {/if}
</section>
{/if}
<div class="statistics-first">{#each [['first_office','clock'],['first_booking','calendar_days']] as [prefix,icon]}{@const fact=firstEvent(values,prefix,period,language,t)}<div title={fact.hint}><Icon name={icon}/><span>{fact.caption}</span><strong>{fact.value}</strong></div>{/each}</div>
{/if}
{#if selected}<div class="statistics-actions"><button class="primary" disabled={actionBusy} onclick={()=>void openAppointment()}><Icon name="check"/>{t('Подтвердить запись')}</button>{#if Number(data?.all.successes??0)>0}<button disabled={actionBusy} onclick={()=>void showAchievement()}><Icon name="award"/>{t('Достижения профиля')}</button>{/if}</div><p class="statistics-purpose muted">{t('После оформления ситы можно сохранить данные записи, добавить её в календарь и поделиться статистикой поиска.')}</p><StatisticsHistory {revision} profile={selected} {period} {language} {t}/>{/if}
{:else if item&&selected}<AchievementPanel profile={selected} {item} {t} {language} {dark} onback={()=>view='stats'} onedit={()=>void openAppointment(true)}/>
{:else}<div class="achievement-empty"><button class="flat-action" onclick={()=>view='stats'}><Icon name="back"/>{t('К статистике')}</button><p class="muted">{t('Пока нет отмеченных сит')}</p><button class="primary" disabled={actionBusy} onclick={()=>void openAppointment()}><Icon name="check"/>{t('Сита получена')}</button></div>{/if}
{#if actionError}<p class="error" role="alert">{t('Не удалось прочитать данные записи')}</p>{/if}
{#if showForm&&context&&selected}<AppointmentDialog profile={selected} {context} saved={editing?item:null} {language} {t} onclose={()=>showForm=false} onsaved={saved}/>{/if}
</section>
<style>
.statistics-panel{width:100%}.statistics-actions{display:flex;gap:12px;margin-top:18px;flex-wrap:wrap}.primary{background:var(--accent);border-color:var(--accent);color:white}.statistics-purpose{font-size:12px;margin:12px 0 18px;line-height:1.5}.achievement-empty{margin-top:20px;display:flex;flex-direction:column;align-items:flex-start;gap:12px}.flat-action{background:none;border:0;padding:5px 0}.showing-achievement .statistics-heading{margin-bottom:18px}.statistics-heading{display:flex;gap:20px;align-items:center;justify-content:space-between}.statistics-heading h1{display:flex;gap:12px;align-items:center;font-size:23px;margin:0}.statistics-profile{width:230px;flex:none}.statistics-period{display:flex;gap:8px;margin:18px 0}.statistics-period button{background:none;border-color:var(--line)}.statistics-period .chosen{background:var(--selection);border-color:var(--accent);color:var(--accent-text)}.statistics-overview{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:16px}.statistics-card{border:1px solid var(--line);background:var(--surface);border-radius:8px;padding:14px 16px;display:flex;flex-direction:column;gap:10px}.statistics-card strong{font-size:28px;font-weight:600;white-space:nowrap}.statistics-card span{display:flex;gap:9px;align-items:center;font-weight:600}.statistics-groups,.statistics-pairs{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:32px}.statistics-groups{margin-top:20px}h2{display:flex;gap:10px;align-items:center;font-size:15px;margin:0 0 6px}.statistics-row{display:flex;gap:10px;align-items:center;padding:12px 0;border-top:1px solid var(--line)}.statistics-row span{flex:1}.statistics-row strong{font-weight:600;font-variant-numeric:tabular-nums}.statistics-effort{margin-top:18px}.statistics-first{margin-top:16px;border-top:1px solid var(--line);padding-top:14px;display:grid;gap:12px}.statistics-first>div{display:flex;gap:10px;align-items:center}.statistics-first span{color:var(--muted);min-width:140px}.statistics-first strong{font-weight:400;font-variant-numeric:tabular-nums}@media(max-width:750px){.statistics-heading{align-items:start}.statistics-heading h1{font-size:20px}.statistics-profile{width:190px}.statistics-overview{gap:10px}.statistics-card{padding:12px}.statistics-card strong{font-size:22px}.statistics-card span{font-size:12px}.statistics-groups,.statistics-pairs{gap:20px}}
</style>
