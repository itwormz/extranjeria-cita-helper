_step => {
  try {
    const buttons = Array.from(document.querySelectorAll('#btnEnviar')).filter(el => {
      const style = getComputedStyle(el), box = el.getBoundingClientRect();
      return el.isConnected && !el.disabled && style.visibility !== 'hidden' && style.visibility !== 'collapse' && box.width > 0 && box.height > 0;
    });
    if (buttons.length !== 1) return 'refused';
    const button = buttons[0];
    const label = button.tagName === 'INPUT' ? button.value : button.innerText;
    if (!/solicitar/i.test(label) || !button.form) return 'refused';
    return button;
  } catch (_) { return 'refused'; }
}
