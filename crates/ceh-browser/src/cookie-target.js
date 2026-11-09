() => {
 const candidates=Array.from(document.querySelectorAll('#cookie-law-info-bar #cookie_action_close_header')).filter(el=>{
  const s=getComputedStyle(el),b=el.getBoundingClientRect();
  return el.isConnected && !el.disabled && s.visibility!=='hidden' && s.visibility!=='collapse' && b.width>0 && b.height>0;
 });
 return candidates.length===1 ? candidates[0] : 'refused';
}
