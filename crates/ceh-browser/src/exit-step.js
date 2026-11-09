step => {
 try {
  const shown=el=>{if(!el || !el.isConnected)return false;const s=getComputedStyle(el),b=el.getBoundingClientRect();return !el.disabled && s.visibility!=='hidden' && s.visibility!=='collapse' && b.width>0 && b.height>0;};
  const label=el=>(el.tagName==='INPUT' ? el.value : el.innerText).trim();
  if(step.mode==='restart'){
   const found=Array.from(document.querySelectorAll('form[action="salirInicio"] #btnSubmit')).filter(el=>shown(el) && /^aceptar$/i.test(label(el)));
   return found.length===1 ? found[0] : 'refused';
  }
  const controls=Array.from(document.querySelectorAll('button, input[type=submit], input[type=button], a')).filter(shown);
  const exits=controls.filter(el=>/salir/i.test(label(el)));
  if(exits.length===1)return exits[0];
  if(exits.length>1)return 'refused';
  if(step.mode==='post_contact' && location.pathname.split('/').pop()==='acOfertarCita'){
   const found=Array.from(document.querySelectorAll('#btnSubmit')).filter(el=>shown(el) && /^aceptar$/i.test(label(el)));
   return found.length===1 ? found[0] : 'refused';
  }
  return 'refused';
 }catch(_){return 'refused';}
}
