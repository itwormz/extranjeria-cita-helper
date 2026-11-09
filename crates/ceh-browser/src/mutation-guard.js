(raw, inspect, operation, allowedOrigin) => {
  // Validate origin before touching private arguments or any page JSON hooks.
  if (!(allowedOrigin ? allowedOrigin(location.origin) : location.origin === 'https://icp.administracionelectronica.gob.es')) return 'refused';
  let step;
  try {
    step = JSON.parse(raw);
    const age = Date.now() - step.issuedAt;
    if (!Number.isFinite(age) || age < 0 || age > 2000) return 'refused';
    const snapshot = JSON.parse(inspect(step.fillMode || 'manual'));
    if (snapshot.frames[0].text !== step.expectedText || snapshot.endpoint !== step.expectedEndpoint || JSON.stringify(snapshot.visible) !== JSON.stringify(step.expectedVisible)) return 'refused';
  } catch (_) { return 'refused'; }
  try { return operation(step); } catch (_) { return 'unknown'; }
}
