'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const Comandos = require('../ui/comandos.js');

// slug: minúsculas, sin tildes, guiones en los separadores y recorte.
test('slug limpia el texto', () => {
  assert.equal(Comandos.slug('Revisión de API!'), 'revision-de-api');
});

// build: seis comandos de serie y agentes propios activos con su prompt.
test('build: de serie y agentes propios', () => {
  assert.equal(Comandos.build().length, 6);
  const activo = { id: 'revisor', name: 'Revisor', enabled: true, write: false, instructions: 'Revisa.' };
  const inactivo = { id: 'fantasma', name: 'Fantasma', enabled: false, instructions: 'No.' };
  const list = Comandos.build([activo, inactivo]);
  const propios = list.filter((c) => c.kind === 'propio');
  assert.equal(propios.length, 1);
  assert.equal(propios[0].name, 'revisor');
  const texto = propios[0].prompt('el login');
  assert.ok(texto.includes('Revisa.'));
  assert.ok(texto.includes('solo lectura'));
  assert.ok(texto.includes('el login'));
});

// build: un agente propio con nombre de serie no lo duplica.
test('el agente propio con nombre de serie no lo duplica', () => {
  const list = Comandos.build([{ id: 'equipo', name: 'Equipo', enabled: true, write: true, instructions: 'X.' }]);
  const eq = list.filter((c) => c.name === 'equipo');
  assert.equal(eq.length, 1);
  assert.equal(eq[0].kind, 'prompt');
});

// build: las plantillas se vuelven comandos de consejo.
test('build añade comandos de plantilla', () => {
  const list = Comandos.build([], [{ id: 't1', name: 'ADR rápido' }]);
  const t = list.find((c) => c.name === 'consejo-adr-rapido');
  assert.ok(t);
  assert.equal(t.kind, 'plantilla');
});

// filter: menú con «/» mientras no haya argumentos todavía.
test('filter lista las opciones del menú', () => {
  const list = Comandos.build();
  assert.equal(Comandos.filter(list, '/').length, 6);
  assert.deepEqual(Comandos.filter(list, '/pi').map((c) => c.name), ['pizarra']);
  for (const c of Comandos.filter(list, '/a')) assert.ok(c.name.includes('a'));
  assert.deepEqual(Comandos.filter(list, '/equipo hola'), []);
  assert.deepEqual(Comandos.filter(list, 'hola'), []);
});

// parse: /nombre argumentos, solo si es un comando de la lista.
test('parse extrae el comando y los argumentos', () => {
  const list = Comandos.build();
  const r = Comandos.parse('/equipo  documenta el MCP ', list);
  assert.equal(r.cmd.name, 'equipo');
  assert.equal(r.args, 'documenta el MCP');
  assert.equal(Comandos.parse('/pizarra', list).args, '');
  assert.equal(Comandos.parse('/noexiste algo', list), null);
  assert.equal(Comandos.parse('hola /equipo', list), null);
  assert.equal(Comandos.parse('/EQUIPO x', list).cmd.name, 'equipo');
});

// Prompts de los comandos de serie.
test('prompts de los comandos de serie', () => {
  const list = Comandos.build();
  const equipo = list.find((c) => c.name === 'equipo');
  assert.ok(equipo.prompt('tarea').includes('agentes run'));
  assert.ok(equipo.prompt('tarea').includes('sin pedir aprobación'));
  const pizarra = list.find((c) => c.name === 'pizarra');
  assert.ok(pizarra.prompt('', { board: 'abc' }).includes('--leer abc'));
  assert.ok(pizarra.prompt('').includes('--leer proyecto'));
});
