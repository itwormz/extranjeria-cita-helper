import {defineConfig} from 'vite';
import {svelte} from '@sveltejs/vite-plugin-svelte';
import {fileURLToPath} from 'node:url';
const local=(path:string)=>fileURLToPath(new URL(path,import.meta.url));
export default defineConfig({
 plugins:[svelte()],publicDir:local('../app/desktop/assets/'),cacheDir:local('../.local/builds/work/rust-vite-cache/'),
 build:{outDir:local('../.local/builds/work/rust-ui-dist/'),emptyOutDir:true},
 server:{host:'127.0.0.1',port:1420,strictPort:true,cors:{origin:'http://127.0.0.1:1420'},fs:{allow:[local('./'),local('../app/desktop/assets/'),local('../.local/builds/work/rust-ui/node_modules/'),local('../.local/builds/work/rust-vite-cache/')],deny:['.env','.env.*','*.{crt,pem}','**/.git/**','**/.local/builds/runtime/**','**/tasks.yml']}},clearScreen:false
});
