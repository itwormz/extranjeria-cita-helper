step => {
 let changed=false;
 try {
  const shown=el=>{if(!el || !el.isConnected)return false;const s=getComputedStyle(el),b=el.getBoundingClientRect();return s.visibility!=='hidden' && s.visibility!=='collapse' && b.width>0 && b.height>0;};
  const unique=id=>{const nodes=Array.from(document.querySelectorAll('[id="'+id+'"]')).filter(shown);return nodes.length===1 ? nodes[0] : null;};
  const fields={phone:unique('txtTelefonoCitado'),email1:unique('emailUNO'),email2:unique('emailDOS')};
  const target=fields[step.field], expected=step.field==='phone' ? step.phone : step.email;
  const usable=el=>el instanceof HTMLInputElement && !el.disabled && !el.readOnly && ['text','tel','email'].includes(el.type);
  const match=(value,expected)=>value.toLowerCase()===expected.toLowerCase();
  if(step.operation==='submit'){
   if(!Object.entries(fields).every(([key,el])=>usable(el) && match(el.value,key==='phone' ? step.phone : step.email)))return 'refused';
   const button=unique('btnSiguiente');if(!button || button.disabled || !button.form || !Object.values(fields).every(el=>el.form===button.form))return 'refused';return button;
  }
  if(!usable(target) || (target.maxLength>=0 && expected.length>target.maxLength))return 'refused';
  if(step.operation==='verify')return match(target.value,expected) ? 'unchanged' : 'refused';
  if(step.operation==='target')return target;
  if(step.operation==='empty')return document.activeElement===target && target.value==='' ? 'unchanged' : 'refused';
  if(step.operation==='ready_key')return document.activeElement===target && match(target.value,step.prefix) ? 'unchanged' : 'refused';
  return 'refused';
 }catch(_){return changed ? 'unknown' : 'refused';}
}
