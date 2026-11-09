() => {
 const facts={portal_entry:false,provider:false,certificate:false,movil:false,permanente:false,requires_attention:false,input_started:false};
 const finish=()=>JSON.stringify(facts);
 if(location.protocol!=='https:' || (location.port && location.port!=='443'))return finish();
 const host=location.hostname;
 const portal=host==='icp.administracionelectronica.gob.es';
 const provider=['pasarela.clave.gob.es','pasarela-ident.clave.gob.es'].includes(host);
 const qr=host==='www2.agenciatributaria.gob.es' && location.pathname.replace(/\/+$/,'').endsWith('/ObtenerClaveMovilQR');
 const clave=host==='clave.gob.es' || host.endsWith('.clave.gob.es');
 if(!portal && !provider && !qr && !clave)return finish();
 const shown=el=>{if(!el || !el.isConnected)return false;const s=getComputedStyle(el),b=el.getBoundingClientRect();return s.visibility!=='hidden' && s.visibility!=='collapse' && b.width>0 && b.height>0;};
 if(portal){facts.portal_entry=Array.from(document.querySelectorAll('#btnAccesoClave')).filter(el=>shown(el) && !el.disabled).length===1;return finish();}
 facts.provider=provider;
 if(provider){
  const controls=Array.from(document.querySelectorAll('button, a, input[type=button], input[type=submit]')).filter(el=>shown(el) && !el.disabled);
  const text=el=>[el.innerText,el.value,el.getAttribute('aria-label'),el.title].filter(Boolean).join(' ').replace(/\s+/g,' ');
  facts.certificate=controls.filter(el=>/certificado electr[oó]nico/i.test(text(el))).length===1;
  facts.movil=controls.filter(el=>/cl@ve m[oó]vil/i.test(text(el))).length===1;
  facts.permanente=controls.filter(el=>/cl@ve permanente/i.test(text(el))).length===1;
 }
 const inputs=Array.from(document.querySelectorAll('input:not([type=hidden]):not([type=submit]):not([type=button]):not([type=checkbox]):not([type=radio])')).filter(el=>shown(el) && !el.disabled && !el.readOnly);
 facts.requires_attention=inputs.length>0;
 // Evaluate only a boolean in the page; never return any credential value.
 facts.input_started=inputs.some(el=>el.value.length>0);
 if(qr && Array.from(document.querySelectorAll('img[src*="qr" i], img[id*="qr" i], img[src^="data:image"], canvas, svg[aria-label*="qr" i]')).some(shown))facts.requires_attention=true;
 return finish();
}
