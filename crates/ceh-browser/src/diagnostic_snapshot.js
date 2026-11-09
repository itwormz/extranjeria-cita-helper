() => {
  if (location.origin !== 'https://icp.administracionelectronica.gob.es') return null;
  if (!/^\/icpplus(?:tieb)?\//.test(location.pathname)) return null;
  const allowed = new Set('main section article div form fieldset legend label input select optgroup option button h1 h2 h3 p ul ol li table tr td th'.split(' '));
  const safeIds = /^(?:form|btnAceptar|btnAccesoClave|btnEnviar|btnSiguiente|btnSubmit|idSede|txtIdCitado|txtCopiar|txtTelefonoCitado|emailUNO|emailDOS|tramiteGrupo\[\d{1,3}\])$/;
  const safeTypes = new Set(['text', 'password', 'submit', 'button', 'checkbox', 'radio', 'email', 'tel', 'date', 'hidden']);
  let count = 0;
  const copy = (source, target, depth) => {
    if (depth > 24 || count >= 2048) return;
    for (const node of source.children) {
      if (++count > 2048) return;
      const tag = node.localName;
      if (['script', 'style', 'iframe', 'object', 'embed', 'template'].includes(tag)) continue;
      if (!allowed.has(tag)) {
        copy(node, target, depth + 1);
        continue;
      }
      const child = document.createElement(tag);
      if (safeIds.test(node.id)) child.id = node.id;
      if (tag === 'input' && safeTypes.has(node.type)) child.type = node.type;
      if (node.disabled === true) child.setAttribute('disabled', '');
      if (node.hidden === true) child.setAttribute('hidden', '');
      target.append(child);
      copy(node, child, depth + 1);
    }
  };
  const shell = document.createElement('body');
  if (document.body) copy(document.body, shell, 0);
  return '<!doctype html><meta charset="utf-8"><title>SantaCita — page structure</title>' + shell.outerHTML;
}
