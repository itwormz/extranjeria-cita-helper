(fillMode) => {
  // Read only. Never focus, scroll, click, capture pixels or read editable values.
  const shown = el => {
    if (!el) return false;
    const style = getComputedStyle(el);
    if (style.visibility === 'hidden' || style.visibility === 'collapse') return false;
    const box = el.getBoundingClientRect();
    return box.width > 0 && box.height > 0;
  };
  const selectors = ['body', '#form', '#btnAccesoClave', '#txtIdCitado', '#btnCopiar', '#btnEnviar',
    '#idSede', "select[name='idSede']", '#txtTelefonoCitado', '#emailUNO', '#emailDOS',
    "input[name*='fecha']", "select[name*='fecha']", "input[name*='hora']", "select[name*='hora']",
    '.ui-datepicker', "input[type='radio'][name*='cita' i]", '#captcha', "input[name*='captcha' i]",
    "img[src*='captcha' i]", 'form[action="salirInicio"] #btnSubmit'];
  const visible = selectors.filter(selector => Array.from(document.querySelectorAll(selector)).some(shown));
  if (Array.from(document.querySelectorAll('select[id^="tramiteGrupo["]')).some(shown)) visible.push('select[id^="tramiteGrupo["]:visible');
  const text = document.body?.innerText ?? '';
  if (text.length > 65536) throw new Error('document inspection limit');
  const applicant = document.querySelector('#txtIdCitado');
  const restart = document.querySelector('form[action="salirInicio"] #btnSubmit');
  const request = document.querySelector('#btnEnviar');
  const button = shown(restart) ? restart : request;
  return JSON.stringify({visible, frames:[{text}], official:location.hostname==='icp.administracionelectronica.gob.es',
    endpoint:location.pathname.split('/').pop() ?? '', fill_mode:fillMode,
    applicant_editable:!!applicant && !applicant.disabled && !applicant.readOnly && applicant.type!=='hidden',
    button_label:button ? (button.getAttribute('value') || button.innerText || '').trim() : '',
    shape:{forms:document.forms.length,selects:document.querySelectorAll('select').length,
      buttons:document.querySelectorAll('button,input[type=submit]').length,
      inputs:['text','password','checkbox','radio','email','tel','date'].map(type=>document.querySelectorAll('input[type="'+type+'"]').length)}
  });
}
