step => {
  try {
    const shown = el => {
      if (!el || !el.isConnected) return false;
      const style=getComputedStyle(el), box=el.getBoundingClientRect();
      return style.visibility!=='hidden' && style.visibility!=='collapse' && box.width>0 && box.height>0;
    };
    const field = id => {const found=Array.from(document.querySelectorAll('[id="'+id+'"]')).filter(shown);return found.length===1 ? found[0] : null;};
    const number=field('txtIdCitado'), name=field('txtDesCitado');
    const nonempty = el => !!el && el.value.trim().length>0;
    const radioId={NIE:'rdbTipoDocNie',DNI:'rdbTipoDocDni',PASSPORT:'rdbTipoDocPas'}[step.documentType];
    // Page scripts only read and resolve elements; every change is made by the browser's own input.
    const textField = () => step.field==='number' ? number : step.field==='name' ? name : null;
    const textValue = () => step.field==='number' ? step.number : step.name;
    const usableText = el => !!el && el instanceof HTMLInputElement && !el.disabled && !el.readOnly && el.type!=='hidden' && typeof textValue()==='string' && !(el.maxLength>=0 && textValue().length>el.maxLength);
    // The page may change letter case while typing (NIE letters, names); compare as `submit` does.
    const match = (a,b) => a.trim().toUpperCase()===b.trim().toUpperCase();
    if (step.operation==='verify_document_type') {
      const radio=radioId ? field(radioId) : null;
      if (!radio) return !step.regreso && !document.querySelector('[name="rdbTipoDoc"]') ? 'unchanged' : 'refused';
      return radio.type==='radio' && !radio.disabled && radio.checked ? 'unchanged' : 'refused';
    }
    if (step.operation==='target_document_type') {
      const radio=radioId ? field(radioId) : null;
      return radio && radio.type==='radio' && !radio.disabled ? radio : 'refused';
    }
    if (step.operation==='verify_copy_ready') return nonempty(number) && nonempty(name) && number.readOnly && name.readOnly ? 'unchanged' : 'refused';
    if (step.operation==='target_copy') {const button=field('btnCopiar');return button && !button.disabled ? button : 'refused';}
    if (step.operation==='verify_copy') return nonempty(number) && nonempty(name) ? 'unchanged' : 'refused';
    if (step.field==='nationality') {
      const select=field('txtPaisNac');if (!select || select.tagName!=='SELECT' || select.disabled) return 'refused';
      const options=Array.from(select.options).filter(option=>option.label===step.nationality && !option.disabled && !(option.parentElement.tagName==='OPTGROUP' && option.parentElement.disabled));
      if (options.length!==1) return 'refused';
      // Python `_select_without_focus`: one input and one change event, no focus, no keyboard or pointer events.
      if (step.operation==='select') {
        if (select.selectedOptions[0]===options[0]) return 'unchanged';
        select.value = options[0].value;
        select.dispatchEvent(new Event('input', {bubbles: true, composed: true}));
        select.dispatchEvent(new Event('change', {bubbles: true}));
        return select.isConnected && select.selectedOptions[0]===options[0] ? 'changed' : 'unknown';
      }
      if (step.operation==='verify') return select.selectedOptions[0]===options[0] ? 'unchanged' : 'refused';
      if (step.operation==='target') return select;
      if (step.operation==='label') return 'label:'+options[0].label;
      return 'refused';
    }
    if (step.field==='number' || step.field==='name') {
      const target=textField();
      if (!usableText(target)) return 'refused';
      if (step.operation==='verify') return match(target.value,textValue()) ? 'unchanged' : 'refused';
      if (step.operation==='target') return target;
      if (step.operation==='focused') return document.activeElement===target ? 'unchanged' : 'refused';
      if (step.operation==='empty') return document.activeElement===target && target.value==='' ? 'unchanged' : 'refused';
      if (step.operation==='ready_key') return document.activeElement===target && match(target.value,step.prefix) ? 'unchanged' : 'refused';
      return 'refused';
    }
    if (step.operation==='submit') {
      if (!nonempty(number) || !nonempty(name)) return 'refused';
      if (!step.copy && (number.value.trim().toUpperCase()!==step.number.trim().toUpperCase() || name.value.trim().toUpperCase()!==step.name.trim().toUpperCase())) return 'refused';
      if (!step.copy) {const radio=radioId ? field(radioId) : null;if ((step.regreso && !radio) || (radio && !radio.checked)) return 'refused';}
      if (!step.regreso) {const select=field('txtPaisNac');if (!select || select.selectedOptions.length!==1 || select.selectedOptions[0].label!==step.nationality) return 'refused';}
      const button=field('btnEnviar');if (!button || button.disabled) return 'refused';
      return button;
    }
    return 'refused';
  } catch (_) {return 'refused';}
}
