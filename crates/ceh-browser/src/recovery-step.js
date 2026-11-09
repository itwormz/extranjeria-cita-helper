step => {
 const regions=Array.from(document.querySelectorAll('div,section,article,dialog,form,body'));
 if(regions.length>256||!Number.isInteger(step.region)||step.region<0||step.region>=regions.length)return 'refused';
 const region=regions[step.region];
 const shown=el=>{const s=getComputedStyle(el),r=el.getBoundingClientRect();return el.isConnected&&s.visibility!=='hidden'&&s.visibility!=='collapse'&&r.width>0&&r.height>0;};
 if(!shown(region)||region.innerText!==step.regionText)return 'refused';
 const candidates=Array.from(region.querySelectorAll("button,input[type='submit'],input[type='button'],a[href]")).filter(el=>{
  if(!shown(el)||el.matches(':disabled')||el.getAttribute('aria-disabled')==='true')return false;
  const label=(el.value||el.innerText||'').trim().replace(/\s+/g,' ');
  return /^(aceptar|volver|salir|reiniciar|reiniciar sesi[oó]n|iniciar|restart|return)$/i.test(label);
 });
 return candidates.length===1?candidates[0]:'refused';
}
