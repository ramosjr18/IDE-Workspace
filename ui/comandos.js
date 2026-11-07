(function (root) {
  // Comandos de serie. kind: 'prompt' (se envía al agente el texto de prompt(args, ctx)), 'consejo' (abre el diálogo de convocar),
  // 'maqueta' (como /maqueta), 'propio' (agente propio), 'plantilla' (consejo con una plantilla).
  // workflow: forma de trabajar solo para ese mensaje (null = la elegida en el menú). requiresArgs: sin texto no hace nada.
  const BUILTIN = [
    { name: 'equipo', args: '<tarea>', description: 'Reparte la tarea entre varios agentes en worktrees y los lanza ya', kind: 'prompt', workflow: 'equipo', requiresArgs: true,
      prompt: (a) => 'Orden directa del usuario: reparte esta tarea entre varios agentes en worktrees y lánzalos ya con `agentes run`, sin pedir aprobación del plan. No hagas commits en la rama del usuario. Tras lanzarlos, termina tu turno diciendo qué has lanzado: la app te avisará cuando acaben.\n\nTarea: ' + a },
    { name: 'consejo', args: '<tema>', description: 'Convoca un consejo de agentes (abre el diálogo)', kind: 'consejo', workflow: null, requiresArgs: false },
    { name: 'diagrama', args: '<qué dibujar>', description: 'Dibuja un diagrama y lo abre en la pizarra', kind: 'prompt', workflow: null, requiresArgs: true,
      prompt: (a) => 'Dibuja en Mermaid un diagrama de: ' + a + '\nEnvíalo a la pizarra con `agentes pizarra --title "<título corto>"` y resume en dos líneas qué muestra.' },
    { name: 'pizarra', args: '[pregunta]', description: 'El agente lee la pizarra abierta y la tiene en cuenta', kind: 'prompt', workflow: null, requiresArgs: false,
      prompt: (a, ctx) => 'Lee la pizarra con `agentes pizarra --leer ' + ((ctx && ctx.board) || 'proyecto') + '` (si falla, usa `agentes pizarra --listar` y elige la que corresponda) y tenla en cuenta.' + (a ? '\n\n' + a : ' Resume qué hay dibujado.') },
    { name: 'maqueta', args: '<tema>', description: 'Maqueta HTML en docs/maquetas/', kind: 'maqueta', workflow: null, requiresArgs: true },
    { name: 'directo', args: '<mensaje>', description: 'Solo este mensaje, sin forma de trabajar', kind: 'prompt', workflow: 'directo', requiresArgs: true, prompt: (a) => a },
  ];

  // Convierte un texto en slug: minúsculas, sin tildes, guiones en los separadores, sin repetidos ni en los bordes, hasta 40.
  function slug(text) {
    return String(text || '')
      .toLowerCase()
      .normalize('NFD')
      .replace(/[\u0300-\u036f]/g, '')
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 40)
      .replace(/-+$/, '');
  }

  // Lista completa de comandos: de serie, más uno por agente propio activo y uno por plantilla (sin duplicar nombres).
  function build(ownAgents = [], templates = []) {
    const list = BUILTIN.slice();
    const names = new Set(list.map((c) => c.name));
    const add = (cmd) => { if (!names.has(cmd.name)) { names.add(cmd.name); list.push(cmd); } };
    for (const a of ownAgents) {
      if (!a.enabled) continue;
      add({ name: slug(a.id), args: '<mensaje>', description: 'Agente propio: ' + a.name, kind: 'propio', workflow: 'directo', requiresArgs: true, agent: a,
        prompt: (x) => a.instructions + (a.write ? '' : '\n\nTrabaja en solo lectura: no modifiques archivos.') + '\n\n' + x });
    }
    for (const t of templates) {
      add({ name: 'consejo-' + slug(t.name), args: '<tema>', description: 'Consejo con la plantilla «' + t.name + '»', kind: 'plantilla', workflow: null, requiresArgs: false, template: t });
    }
    return list;
  }

  // Opciones del menú: nada si no empieza por «/» o ya hay argumentos; primero por prefijo, luego por inclusión, hasta 10.
  function filter(list, text) {
    if (!text || text[0] !== '/' || /\s/.test(text)) return [];
    const q = text.slice(1).toLowerCase();
    const starts = [];
    const rest = [];
    for (const c of list) {
      const n = c.name.toLowerCase();
      if (n.startsWith(q)) starts.push(c);
      else if (n.includes(q)) rest.push(c);
    }
    return starts.concat(rest).slice(0, 10);
  }

  // Extrae el comando de un mensaje: /nombre argumentos. Null si no casa o no es un comando de la lista.
  function parse(message, list) {
    const m = /^\/([a-z0-9][a-z0-9-]*)(?:\s+([\s\S]*))?$/i.exec(String(message || ''));
    if (!m) return null;
    const cmd = list.find((c) => c.name === m[1].toLowerCase());
    return cmd ? { cmd, args: (m[2] || '').trim() } : null;
  }

  const api = { BUILTIN, slug, build, filter, parse };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.Comandos = api;
})(typeof window !== "undefined" ? window : globalThis);
