'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { mockupSlug, mockupPath, mockupPrompt } = require('../ui/maquetas.js');

// Texto íntegro del prompt cuando no hay decisión aprobada que anclar.
const PROMPT_COMPLETO = [
  'Crea una maqueta en HTML para: Tema X',
  'Guárdala en docs/maquetas/tema-x.html (crea la carpeta si no existe). Si el archivo ya existe, edítalo en lugar de crear otro.',
  'Requisitos:',
  '- Un único archivo HTML autocontenido: CSS y JavaScript en línea, sin recursos externos (ni CDNs, ni fuentes, ni imágenes remotas). Si necesitas imágenes, usa SVG en línea.',
  '- Textos de ejemplo realistas y en español.',
  '- Que se vea bien en un panel estrecho y a pantalla completa.',
  '- Marca con comentarios HTML las partes que sean supuestos tuyos.',
  'Al terminar, resume en pocas líneas qué pantallas o estados incluye la maqueta.',
].join('\n');

test('slug: tildes, signos y espacios', () => {
  assert.strictEqual(mockupSlug('Pantalla de acceso (móvil)!'), 'pantalla-de-acceso-movil');
});

test('slug: sin guiones repetidos ni en los extremos', () => {
  assert.strictEqual(mockupSlug('  --Hola   mundo-- '), 'hola-mundo');
  assert.strictEqual(mockupSlug('Mi--tema--de--prueba'), 'mi-tema-de-prueba');
});

test('slug: vacío cae a «maqueta»', () => {
  assert.strictEqual(mockupSlug(''), 'maqueta');
  assert.strictEqual(mockupSlug('   --  !! '), 'maqueta');
});

test('slug: corte a 60 caracteres sin guion final', () => {
  const largo = mockupSlug('a'.repeat(61));
  assert.strictEqual(largo.length, 60);
  assert.ok(!largo.endsWith('-'));
  // Si el guion cae justo en el corte, se recorta también.
  const conGuionAlCorte = mockupSlug('a'.repeat(59) + ' bb');
  assert.strictEqual(conGuionAlCorte, 'a'.repeat(59));
  assert.ok(!conGuionAlCorte.endsWith('-'));
});

test('mockupPath', () => {
  assert.strictEqual(mockupPath('Pantalla de acceso (móvil)!'), 'docs/maquetas/pantalla-de-acceso-movil.html');
});

test('mockupPrompt sin documentPath: no menciona la decisión y el texto es exacto', () => {
  const p = mockupPrompt(' Tema X ');
  assert.ok(!p.includes('Basa la maqueta'));
  assert.strictEqual(p, PROMPT_COMPLETO);
});

test('mockupPrompt con documentPath: incluye la ruta del documento y la de la maqueta', () => {
  const p = mockupPrompt('Tema X', 'docs/decisiones/adr-001.md');
  assert.ok(p.includes('Basa la maqueta en la decisión aprobada en docs/decisiones/adr-001.md: léela antes de empezar.'));
  assert.ok(p.includes('docs/decisiones/adr-001.md'));
  assert.ok(p.includes(mockupPath('Tema X')));
});
