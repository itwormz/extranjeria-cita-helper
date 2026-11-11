<script lang="ts">
import {onMount} from 'svelte';import {legacyImport} from './bridge';import {searchErrors} from './work-state';
let {name,t,onclose,onimported}:{name:string;t:(key:string)=>string;onclose:()=>void;onimported:()=>void}=$props();
let dialog:HTMLDialogElement;let busy=$state(false);let failed=$state('');
onMount(()=>{dialog.showModal();return()=>dialog.close();});
async function run(){if(busy)return;busy=true;failed='';try{await legacyImport(name);onimported();onclose();}catch(e){failed=searchErrors[String(e)]??'Не удалось импортировать данные.';}finally{busy=false;}}
</script>
<dialog class="import-offer" bind:this={dialog} oncancel={e=>{e.preventDefault();if(!busy)onclose();}}>
<h2>{t('Найдены данные прошлой версии')}</h2>
<p>{t('Профили, статистику и достижения из «{name}» можно перенести в SantaCita. Старые файлы не изменятся.').replace('{name}',name)}</p>
{#if failed}<p class="error" role="alert">{t(failed)}</p>{/if}
<div class="actions"><button type="button" onclick={onclose} disabled={busy}>{t('Не сейчас')}</button><button type="button" class="primary" onclick={()=>void run()} disabled={busy}>{t('Импортировать')}</button></div>
</dialog>
<style>
.import-offer{width:min(480px,calc(100vw - 32px));padding:22px;border:1px solid var(--strong);border-radius:12px;background:var(--bg);color:var(--text)}.import-offer::backdrop{background:#0008}h2{font-size:19px;margin:0 0 10px}p{margin:0 0 10px;overflow-wrap:anywhere}.actions{display:flex;justify-content:flex-end;gap:10px;margin-top:14px}
</style>
