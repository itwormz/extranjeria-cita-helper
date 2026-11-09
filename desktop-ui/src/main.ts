import {mount} from 'svelte';
import App from './App.svelte';
import {guardWheel} from './wheel';
import './theme.css';
document.addEventListener('wheel', guardWheel, {capture: true, passive: true});
mount(App,{target:document.getElementById('app')!});
