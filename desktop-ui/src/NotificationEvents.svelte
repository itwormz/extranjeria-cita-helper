<script lang="ts">
import type {NotificationEvent} from './notifications';
let {values=$bindable<NotificationEvent[]>([]),t,onchange}:{values:NotificationEvent[];t:(key:string)=>string;onchange:()=>void}=$props();
const options:{value:NotificationEvent;label:string}[]=[{value:'auth',label:'Авторизация'},{value:'office',label:'Выбор офиса'},{value:'booking',label:'Выбор ситы'},{value:'error',label:'Остановка с ошибкой'}];
let expanded=$state(false);let all=$derived(options.every(o=>values.includes(o.value)));
function toggle(key:NotificationEvent,checked:boolean){values=checked?[...values.filter(v=>v!==key),key]:values.filter(v=>v!==key);onchange();}
</script>
<div class="notification-events"><div class="notification-events-heading"><label><input type="checkbox" checked={all} onchange={e=>{values=e.currentTarget.checked?options.map(o=>o.value):[];onchange();}}/>{t('Все важные события')}</label><button type="button" class="disclosure" aria-expanded={expanded} onclick={()=>expanded=!expanded}>{expanded?'▾':'▸'} {t('Настроить')}</button></div>{#if expanded}<div class="notification-event-options">{#each options as option}<label><input type="checkbox" checked={values.includes(option.value)} onchange={e=>toggle(option.value,e.currentTarget.checked)}/>{t(option.label)}</label>{/each}</div>{/if}<small class="muted">{t('Только когда требуется участие или поиск остановлен.')}</small></div>
