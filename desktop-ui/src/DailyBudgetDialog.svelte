<script lang="ts">
import {onMount,untrack} from 'svelte';import Icon from './Icon.svelte';import {parseDailyTarget,sourceKey,type BudgetSummary} from './budget';import {budgetErrorText} from './work-state';
let {budget,t,onclose,onsave}:{budget:BudgetSummary;t:(key:string)=>string;onclose:()=>void;onsave:(target:number|null)=>Promise<void>}=$props();
let value=$state(untrack(()=>String(budget.target)));let busy=$state(false);let error=$state('');let dialog:HTMLDialogElement;let field:HTMLInputElement;
const parsed=$derived(parseDailyTarget(value));const remaining=$derived(Math.max(0,(parsed??budget.target)-budget.sent));
const adaptive=$derived(budget.adaptive_enabled&&budget.adaptive_target!==null);
onMount(()=>{dialog.showModal();field.focus();field.select();return()=>dialog.close();});
async function save(target:number|null){if(busy)return;busy=true;error='';try{await onsave(target);}catch(e){error=budgetErrorText(e);}finally{busy=false;}}
</script>
<dialog class="daily-budget" bind:this={dialog} oncancel={e=>{e.preventDefault();if(!busy)onclose();}}>
<form onsubmit={e=>{e.preventDefault();if(parsed!==null)void save(parsed);}}>
<h2><Icon name="clock"/>{t('Бюджет на сегодня')}</h2>
<div class="meta"><span class="muted">{t('До полуночи')}</span><span class="muted">{t(sourceKey(budget))}</span></div>
<div class="row"><label class="profile-field"><span><small>{t('Всего попыток на день')}</small><input bind:this={field} aria-label={t('Всего попыток на день')} bind:value inputmode="numeric" maxlength="10" autocomplete="off" aria-invalid={parsed===null}/></span></label>
<dl class="counts"><dt class="muted">{t('Отправлено')}</dt><dd>{budget.sent}</dd><dt class="muted">{t('Осталось')}</dt><dd data-testid="remaining">{remaining}</dd></dl></div>
{#if parsed===null}<p class="field-error" role="alert">{t('Бюджет должен быть целым числом от 1 до 2 147 483 647.')}</p>{/if}
{#if budget.adaptive_enabled}<p class="muted">{t('Адаптивный бюджет продолжает рассчитываться на завтра.')}</p>{/if}
{#if error}<p class="error" role="alert">{t(error)}</p>{/if}
<div class="actions">{#if budget.target_source==='manual'}<button type="button" disabled={busy} onclick={()=>void save(null)}>{t(adaptive?'Вернуть адаптивный':'Вернуть базовый')}</button>{/if}<span class="spacer"></span><button type="button" disabled={busy} onclick={onclose}>{t('Отмена')}</button><button class="primary" type="submit" disabled={busy||parsed===null}><Icon name="check"/>{t('Сохранить')}</button></div>
</form></dialog>
<style>
.daily-budget{width:min(510px,calc(100vw - 32px));padding:22px;border:1px solid var(--strong);border-radius:12px;background:var(--bg);color:var(--text)}.daily-budget::backdrop{background:#0008}h2{display:flex;align-items:center;gap:10px;font-size:19px;margin:0 0 12px}.meta{display:flex;justify-content:space-between;gap:12px;margin-bottom:14px;font-size:12px}.row{display:flex;gap:24px;align-items:flex-start}.row .profile-field{flex:1;min-width:0}.counts{display:grid;grid-template-columns:auto auto;gap:5px 16px;margin:0;align-items:baseline}.counts dd{margin:0;text-align:right;font-variant-numeric:tabular-nums}.actions{display:flex;gap:10px;margin-top:18px;flex-wrap:wrap}.spacer{flex:1}.field-error{color:var(--danger,#e4685d);font-size:12px;margin:8px 0 0}
</style>
