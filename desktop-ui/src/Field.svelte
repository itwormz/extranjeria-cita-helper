<script lang="ts">
import Icon from './Icon.svelte';
let {label,icon='text',value=$bindable(''),type='text',maxlength=256}: {label:string;icon?:string;value?:string;type?:string;maxlength?:number}=$props();
</script>
<label class="profile-field"><Icon name={icon}/><span><small>{label}</small><input aria-label={label} {type} {maxlength} bind:value autocomplete="off" spellcheck="false"/></span></label>
