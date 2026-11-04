(function (root) {
  // Convierte un tema en slug seguro: minúsculas, sin tildes, guiones en lugar de caracteres no alfanuméricos y sin repetidos.
  function mockupSlug(topic) {
    return String(topic || '')
      .toLowerCase()
      .normalize('NFD')
      .replace(/[\u0300-\u036f]/g, '')
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 60)
      .replace(/-+$/g, '') || 'maqueta';
  }

  // Ruta del archivo HTML donde se guarda la maqueta del tema.
  function mockupPath(topic) {
    return 'docs/maquetas/' + mockupSlug(topic) + '.html';
  }

  // Texto que se envía al agente del chat para que genere la maqueta; con documentPath lo ancla a la decisión aprobada.
  function mockupPrompt(topic, documentPath) {
    const tema = String(topic || '').trim();
    const doc = String(documentPath || '').trim();
    return [
      'Crea una maqueta en HTML para: ' + tema,
      doc ? 'Basa la maqueta en la decisión aprobada en ' + doc + ': léela antes de empezar.' : null,
      'Guárdala en ' + mockupPath(tema) + ' (crea la carpeta si no existe). Si el archivo ya existe, edítalo en lugar de crear otro.',
      'Requisitos:',
      '- Un único archivo HTML autocontenido: CSS y JavaScript en línea, sin recursos externos (ni CDNs, ni fuentes, ni imágenes remotas). Si necesitas imágenes, usa SVG en línea.',
      '- Textos de ejemplo realistas y en español.',
      '- Que se vea bien en un panel estrecho y a pantalla completa.',
      '- Marca con comentarios HTML las partes que sean supuestos tuyos.',
      'Al terminar, resume en pocas líneas qué pantallas o estados incluye la maqueta.',
    ].filter((l) => l !== null).join('\n');
  }

  const api = { mockupSlug, mockupPath, mockupPrompt };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.Maquetas = api;
})(typeof window !== "undefined" ? window : globalThis);
