<script lang="ts">
import {onMount} from 'svelte';import {historyLines,type BudgetSummary} from './budget';
let {budget,t,onclose}:{budget:BudgetSummary;t:(key:string)=>string;onclose:()=>void}=$props();
let dialog:HTMLDialogElement;const lines=$derived(historyLines(budget,t));
onMount(()=>{dialog.showModal();return()=>dialog.close();});
</script>
<dialog class="budget-history" bind:this={dialog} oncancel={e=>{e.preventDefault();onclose();}}>
<h2>{t('История возможных ограничений')}</h2>
<div class="lines" tabindex="0">{#if lines.length}{#each lines as line}<p>{line||' '}</p>{/each}{:else}<p class="muted">{t('Эпизодов пока нет')}</p>{/if}</div>
<div class="actions"><button type="button" onclick={onclose}>{t('Закрыть')}</button></div>
</dialog>
<style>
.budget-history{width:min(560px,calc(100vw - 32px));max-height:calc(100vh - 48px);padding:22px;border:1px solid var(--strong);border-radius:12px;background:var(--bg);color:var(--text)}.budget-history::backdrop{background:#0008}h2{font-size:19px;margin:0 0 12px}.lines{max-height:320px;overflow:auto;user-select:text}.lines p{margin:0;font-variant-numeric:tabular-nums;overflow-wrap:anywhere}.actions{display:flex;justify-content:flex-end;margin-top:14px}
</style>
