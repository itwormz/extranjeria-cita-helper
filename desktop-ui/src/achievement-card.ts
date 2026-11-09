import type {PublicCard} from './bridge';import paths from './icons.json';
/** Only the Rust public model enters this renderer; no appointment or profile object. */
export async function renderCard(model:PublicCard,dark:boolean):Promise<HTMLCanvasElement>{
 await document.fonts.ready;const canvas=document.createElement('canvas');canvas.width=1200;canvas.height=900;const ctx=canvas.getContext('2d');if(!ctx)throw new Error('canvas_unavailable');
 const bg=dark?'#26272C':'#FFFFFF',text=dark?'#E8E8E8':'#282A30',muted=dark?'#9A9B9F':'#6F6F6F',line=dark?'#3A3B40':'#E6E6E6';
 ctx.fillStyle=bg;ctx.fillRect(0,0,1200,900);ctx.fillStyle='#38AD84';ctx.fillRect(0,0,1200,7);
 const icon=async(name:string,x:number,y:number,size:number)=>{const image=new Image();image.src='data:image/svg+xml;charset=utf-8,'+encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="${text}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">${paths[name as keyof typeof paths]??paths.list}</svg>`);await image.decode();ctx.drawImage(image,x,y,size,size);};
 await icon('calendar',64,54,38);ctx.fillStyle=text;ctx.font='600 38px Plex';ctx.fillText(model.title,119,88);ctx.font='600 48px Plex';ctx.fillText(model.duration,64,172);
 const icons=['search','clock','office','pin','calendar','calendar_days','server','clock','shield','restart','pointer','file_text'];
 for(let i=0;i<model.rows.length;i++){const column=i%2,row=Math.floor(i/2),x=64+column*560,y=232+row*88;ctx.strokeStyle=line;ctx.beginPath();ctx.moveTo(x,y);ctx.lineTo(x+512,y);ctx.stroke();await icon(icons[i]??'list',x,y+25,25);const [label,value]=model.rows[i];ctx.fillStyle=text;ctx.font='400 24px Plex';let caption=label;while(ctx.measureText(caption).width>320&&caption.length>1)caption=caption.slice(0,-1);if(caption!==label)caption=caption.slice(0,-1)+'…';ctx.fillText(caption,x+40,y+46);ctx.font='600 25px Plex';ctx.textAlign='right';ctx.fillText(value,x+512,y+46);ctx.textAlign='left';}
 ctx.fillStyle=muted;ctx.font='400 23px Plex';ctx.fillText(model.credit,64,838);return canvas;
}
export async function pngBytes(canvas:HTMLCanvasElement):Promise<number[]>{const blob=await new Promise<Blob>((resolve,reject)=>canvas.toBlob(value=>value?resolve(value):reject(new Error('png_unavailable')),'image/png'));return Array.from(new Uint8Array(await blob.arrayBuffer()));}
