<script lang="ts">
import paths from './icons.json';
let {name,size=20}: {name:string;size?:number}=$props();
</script>
<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@html paths[name as keyof typeof paths]??paths.list}</svg>
