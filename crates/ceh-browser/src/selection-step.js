step => {
  let changed = false;
  try {
    const shown = el => {
      if (!el || !el.isConnected) return false;
      const style = getComputedStyle(el), box = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.visibility !== 'collapse' && box.width > 0 && box.height > 0;
    };
    const selectors = step.office ? '#idSede, select[name="idSede"]' : step.province ? '#form' : 'select[id^="tramiteGrupo["]';
    const choices = [];
    for (const select of document.querySelectorAll(selectors)) {
      if (select.tagName !== 'SELECT' || select.disabled || !shown(select)) continue;
      for (const option of select.options) {
        if ((step.office ? (!['','0','-1'].includes(option.value.trim()) && !/selecci[oó]n|seleccione|seleccionar|elija|cualquier oficina/i.test(option.text)) : step.labels.includes(option.label)) && !option.disabled && !(option.parentElement.tagName === 'OPTGROUP' && option.parentElement.disabled)) choices.push({select,option});
      }
    }
    if (choices.length !== 1) return 'refused';
    const {select, option} = choices[0];
    if (!step.submit) {
      if (!step.prepare) return 'refused';
      if (select.selectedOptions[0] === option) return 'completed';
      changed = true;
      select.selectedIndex = option.index;
      select.dispatchEvent(new Event('input', {bubbles: true}));
      select.dispatchEvent(new Event('change', {bubbles: true}));
      return select.isConnected && select.selectedOptions[0] === option ? 'completed' : 'unknown';
    }
    if (select.selectedOptions[0] !== option) return 'refused';
    const buttons = Array.from(document.querySelectorAll(step.office ? 'button, input[type=submit], input[type=button], a' : '#btnAceptar')).filter(el => shown(el) && !el.disabled && (!step.office || /(?:siguiente|aceptar|continuar|confirmar)/i.test(el.tagName === 'INPUT' ? el.value : el.innerText)));
    if (step.office && (!select.form || buttons.length !== 1 || buttons[0].form !== select.form)) return 'refused';
    if (buttons.length !== 1 || (select.form && buttons[0].form && select.form !== buttons[0].form)) return 'refused';
    return buttons[0];
  } catch (_) {
    return changed ? 'unknown' : 'refused';
  }
}
