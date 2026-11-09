step => {
 try {
  const pattern={certificate:/certificado electr[oó]nico/i,movil:/cl@ve m[oó]vil/i,permanente:/cl@ve permanente/i}[step.method];
  if(!pattern)return 'refused';
  const shown=el=>{const s=getComputedStyle(el),b=el.getBoundingClientRect();return el.isConnected && !el.disabled && s.visibility!=='hidden' && s.visibility!=='collapse' && b.width>0 && b.height>0;};
  const text=el=>[el.innerText,el.value,el.getAttribute('aria-label'),el.title].filter(Boolean).join(' ').replace(/\s+/g,' ');
  const found=Array.from(document.querySelectorAll('button, a, input[type=button], input[type=submit]')).filter(el=>shown(el) && pattern.test(text(el)));
  return found.length===1 ? found[0] : 'refused';
 }catch(_){return 'refused';}
}
