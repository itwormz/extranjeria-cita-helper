() => {
 if(location.origin!=='https://icp.administracionelectronica.gob.es')throw new Error('unsupported origin');
 // Known radio labels and calendar cells only; time dropdowns are search filters, not offers (Python 0.3.6).
 const visible=el=>!!el && el.getClientRects().length>0 && getComputedStyle(el).visibility==='visible' && getComputedStyle(el).display!=='none' && !el.closest('[hidden], [inert], [aria-hidden="true"]');
 const labels=Array.from(document.querySelectorAll('input[type="radio"][name="rdbCita" i]'))
  .filter(el=>!el.matches(':disabled') && visible(el)).slice(0,32)
  .map(el=>Array.from(el.labels||[]).filter(visible).map(label=>label.innerText).join(' ').slice(0,240));
 const table=document.querySelector('#VistaMapa_Datatable');
 if(!visible(table))return JSON.stringify(labels);
 const dates=new Map();
 for(const header of Array.from(table.querySelectorAll('thead th')).slice(0,32)){
  const column=Array.from(header.classList).find(name=>/^colFecha\d+$/.test(name));
  const date=header.innerText.match(/\b\d{2}\/\d{2}\/\d{4}\b/);
  if(column && date && visible(header))dates.set(column,date[0]);
 }
 for(const row of Array.from(table.querySelectorAll('tbody > tr')).slice(0,256)){
  const heading=row.querySelector(':scope > th');
  if(!visible(row) || !visible(heading))continue;
  const time=heading.innerText.trim();
  if(!/^(?:[01]\d|2[0-3]):[0-5]\d$/.test(time))continue;
  for(const cell of Array.from(row.querySelectorAll(':scope > td')).slice(0,32)){
   if(labels.length>=32)return JSON.stringify(labels);
   const column=Array.from(cell.classList).find(name=>dates.has(name));
   if(!column || !visible(cell) || cell.closest('[disabled], [aria-disabled="true"]'))continue;
   const free=Array.from(cell.querySelectorAll('a[role="button"]')).slice(0,8).some(link=>visible(link) && !link.closest('[disabled], [aria-disabled="true"]') && link.innerText.trim().toUpperCase()==='LIBRE');
   if(free)labels.push('Día: '+dates.get(column)+' Hora: '+time);
  }
 }
 return JSON.stringify(labels);
}
