_step => {
 const visible = el => {
  const style = getComputedStyle(el), box = el.getBoundingClientRect();
  return el.isConnected && !el.disabled && el.getAttribute('aria-disabled') !== 'true'
   && style.visibility === 'visible' && box.width > 0 && box.height > 0;
 };
 const entries = Array.from(document.querySelectorAll('#btnAccesoClave')).filter(visible);
 if (entries.length !== 1) return 'refused';
 const entry = entries[0];
 const controls = Array.from(entry.querySelectorAll('button, a[href], input[type=submit], input[type=button], [role=button]')).filter(visible);
 if (controls.length > 1) return 'refused';
 return controls[0] ?? entry;
}
