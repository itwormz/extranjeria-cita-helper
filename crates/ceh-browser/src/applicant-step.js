step => {
  let changed = false;
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
    if (step.operation==='document_type') {
      const radio=radioId ? field(radioId) : null;
      if (!radio) return !step.regreso && !document.querySelector('[name="rdbTipoDoc"]') ? 'unchanged' : 'refused';
      if (radio.disabled || radio.type!=='radio') return 'refused';
      if (radio.checked) return 'unchanged';
      changed=true;radio.click();return radio.checked ? 'changed' : 'unknown';
    }
    if (step.operation==='copy') {
      if (nonempty(number) && nonempty(name) && number.readOnly && name.readOnly) return 'unchanged';
      const button=field('btnCopiar');if (!button || button.disabled) return 'refused';
      changed=true;button.click();return 'changed';
    }
    if (step.operation==='verify_copy') return nonempty(number) && nonempty(name) ? 'unchanged' : 'refused';
    if (step.operation==='number' || step.operation==='name') {
      const target=step.operation==='number' ? number : name, value=step.operation==='number' ? step.number : step.name;
      if (!target || target.disabled || target.readOnly || target.type==='hidden' || typeof value!=='string' || (target.maxLength>=0 && value.length>target.maxLength)) return 'refused';
      if (target.value===value) return 'unchanged';
      const prototype=target instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      const setter=Object.getOwnPropertyDescriptor(prototype,'value')?.set;if (!setter) return 'refused';
      changed=true;setter.call(target,value);
      target.dispatchEvent(new InputEvent('input',{bubbles:true,composed:true,inputType:'insertText',data:value}));
      target.dispatchEvent(new Event('change',{bubbles:true}));
      return 'changed';
    }
    if (step.operation==='nationality') {
      const select=field('txtPaisNac');if (!select || select.tagName!=='SELECT' || select.disabled) return 'refused';
      const options=Array.from(select.options).filter(option=>option.label===step.nationality && !option.disabled && !(option.parentElement.tagName==='OPTGROUP' && option.parentElement.disabled));
      if (options.length!==1) return 'refused';if (select.value===options[0].value) return 'unchanged';
      changed=true;select.value=options[0].value;select.dispatchEvent(new Event('input',{bubbles:true,composed:true}));select.dispatchEvent(new Event('change',{bubbles:true}));return 'changed';
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
  } catch (_) {return changed ? 'unknown' : 'refused';}
}
