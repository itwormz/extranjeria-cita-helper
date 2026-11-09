<script lang="ts">
import StatisticsPanel from '../src/StatisticsPanel.svelte';import {translate} from '../src/i18n';import {setStatisticsScenario} from './notification-preview-bridge';
let language=$state('ru');let dark=$state(true);let version=$state(0);let selected=$state('fixture');
const profiles=[{id:'fixture',label:'Fixture Profile',procedure:'huellas',province:'Barcelona',valid:true},{id:'second',label:'Fixture Two',procedure:'regreso',province:'Madrid',valid:true}];const t=(key:string)=>translate(key,language);
</script>
<div class="app-shell" class:light={!dark}><header style="padding:12px;align-items:center;height:auto">{#each ['ru','en','es'] as code}<button onclick={()=>language=code}>{code.toUpperCase()}</button>{/each}<button onclick={()=>dark=!dark}>Light / Dark</button>{#each ['full','legacy','empty','error'] as scenario}<button onclick={()=>{setStatisticsScenario(scenario);version++;}}>{scenario}</button>{/each}</header><main>{#key version}<StatisticsPanel {profiles} {selected} onselect={id=>selected=id} {language} {t} {dark}/>{/key}<small>Statistics fixture only — synthetic data, no Tauri or credentials.</small></main></div>
