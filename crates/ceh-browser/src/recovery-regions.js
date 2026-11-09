() => {
 if(location.origin!=='https://icp.administracionelectronica.gob.es')return '[]';
 // Bound traversal and returned text; never read editable values.
 const walker=document.createTreeWalker(document,NodeFilter.SHOW_ELEMENT);
 let count=0;while(walker.nextNode())if(++count>4096)throw new Error('region limit');
 const containers=Array.from(document.querySelectorAll('div,section,article,dialog,form,body'));
 if(containers.length>256)throw new Error('region limit');
 const shown=el=>{const s=getComputedStyle(el),r=el.getBoundingClientRect();return s.visibility!=='hidden'&&s.visibility!=='collapse'&&r.width>0&&r.height>0;};
 const rows=[];let total=0;
 for(let index=0;index<containers.length;index++){
  const region=containers[index];if(!shown(region))continue;
  const buttons=Array.from(region.querySelectorAll("button,input[type='submit'],input[type='button'],a[href]")).filter(el=>shown(el)&&!el.matches(':disabled')&&el.getAttribute('aria-disabled')!=='true');
  const choices=buttons.map(el=>(el.value||el.innerText||'').trim().replace(/\s+/g,' ')).filter(label=>/^(aceptar|volver|salir|reiniciar|reiniciar sesi[oó]n|iniciar|restart|return)$/i.test(label));
  if(choices.length!==1)continue;
  const text=region.innerText;if(text.length>65536||(total+=text.length)>65536)throw new Error('region text limit');
  rows.push({index,size:region.querySelectorAll('*').length,text,label:choices[0]});
 }
 return JSON.stringify(rows);
}
