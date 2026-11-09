() => {
 if(location.origin!=='https://icp.administracionelectronica.gob.es')throw new Error('unsupported origin');
 const shown=el=>{const s=getComputedStyle(el),b=el.getBoundingClientRect();return s.visibility!=='hidden' && s.visibility!=='collapse' && b.width>0 && b.height>0;};
 const selects=Array.from(document.querySelectorAll("#idSede, select[name='idSede']")).filter(shown);
 if(selects.length!==1)throw new Error('ambiguous office control');
 const select=selects[0];
 const options=Array.from(select.options).filter(o=>!o.disabled && !o.parentElement.disabled && !['','0','-1'].includes(o.value.trim()) && !/selecci[oó]n|seleccione|seleccionar|elija|cualquier oficina/i.test(o.text));
 if(options.length>128 || options.some(o=>o.text.length>320))throw new Error('office label limit');
 // Option identifiers stay in the page. Only public display labels are returned.
 const selected=options.find(o=>o.selected);
 return JSON.stringify({offices:options.map(o=>o.text),selected:selected ? selected.text : null});
}
