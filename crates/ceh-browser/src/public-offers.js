() => {
 if(location.origin!=='https://icp.administracionelectronica.gob.es')throw new Error('unsupported origin');
 const visible=el=>!!el && el.getClientRects().length>0 && getComputedStyle(el).visibility==='visible' && getComputedStyle(el).display!=='none';
 return JSON.stringify(Array.from(document.querySelectorAll('input[type="radio"][name="rdbCita" i]'))
  .filter(el=>!el.matches(':disabled') && visible(el)).slice(0,32)
  .map(el=>Array.from(el.labels||[]).filter(visible).map(label=>label.innerText).join(' ').slice(0,240)));
}
