'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { councilStats, formatDuration, formatTokens, formatStats } = require('../ui/consejo-coste.js');

const call = (agent, agentName, extra) => ({ agent, agentName, phase: 'diverge', round: 1, durationMs: 1000, ok: true, ...extra });

test('councilStats: totales y orden de byAgent con dos agentes', () => {
  const c = councilStats({ calls: [
    call('a', 'Ana', { usage: { inputTokens: 100, outputTokens: 50, costUsd: 0.1, requests: 2 } }),
    call('b', 'Beto', { durationMs: 500 }),
    call('a', 'Ana', { durationMs: 2000 }),
  ] });
  assert.deepStrictEqual(c.total, { calls: 3, failed: 0, durationMs: 3500, inputTokens: 100, outputTokens: 50, costUsd: 0.1, requests: 2 });
  assert.equal(c.byAgent.length, 2);
  assert.deepEqual(Object.keys(c.byAgent[0]).slice(0, 2), ['agent', 'agentName']);
  assert.equal(c.byAgent[0].agent, 'a');
  assert.equal(c.byAgent[0].agentName, 'Ana');
  assert.equal(c.byAgent[0].calls, 2);
  assert.equal(c.byAgent[0].durationMs, 3000);
  assert.equal(c.byAgent[1].agent, 'b');
  assert.equal(c.byAgent[1].calls, 1);
  assert.equal(c.byAgent[1].inputTokens, null);
});

test('councilStats: null cuando nadie trae el dato y suma cuando alguno sí', () => {
  const empty = councilStats({ calls: [call('a', 'Ana'), call('b', 'Beto')] });
  assert.equal(empty.total.inputTokens, null);
  assert.equal(empty.total.outputTokens, null);
  assert.equal(empty.total.costUsd, null);
  assert.equal(empty.total.requests, null);
  const some = councilStats({ calls: [
    call('a', 'Ana', { usage: { inputTokens: 10 } }),
    call('b', 'Beto'),
    call('b', 'Beto', { usage: { inputTokens: 20, outputTokens: 5 } }),
  ] });
  assert.equal(some.total.inputTokens, 30);
  assert.equal(some.total.outputTokens, 5);
  assert.equal(some.byAgent[0].inputTokens, 10);
  assert.equal(some.byAgent[1].inputTokens, 20);
  assert.equal(some.byAgent[1].outputTokens, 5);
  assert.equal(some.byAgent[0].outputTokens, null);
});

test('councilStats: failed cuenta las llamadas con ok === false', () => {
  const c = councilStats({ calls: [call('a', 'Ana', { ok: false }), call('a', 'Ana'), call('b', 'Beto', { ok: false })] });
  assert.equal(c.total.calls, 3);
  assert.equal(c.total.failed, 2);
  assert.equal(c.byAgent[0].failed, 1);
  assert.equal(c.byAgent[1].failed, 1);
});

test('councilStats: consejo sin calls', () => {
  const empty = { calls: 0, failed: 0, durationMs: 0, inputTokens: null, outputTokens: null, costUsd: null, requests: null };
  for (const c of [null, {}, { calls: null }]) {
    const s = councilStats(c);
    assert.deepStrictEqual(s.total, empty);
    assert.deepEqual(s.byAgent, []);
  }
});

test('formatDuration: los tres rangos', () => {
  assert.equal(formatDuration(45000), '45 s');
  assert.equal(formatDuration(125000), '2 min 5 s');
  assert.equal(formatDuration(120000), '2 min');
  assert.equal(formatDuration(3720000), '1 h 2 min');
});

test('formatTokens: los tres rangos', () => {
  assert.equal(formatTokens(950), '950');
  assert.equal(formatTokens(3000), '3 k');
  assert.equal(formatTokens(45200), '45,2 k');
  assert.equal(formatTokens(1200000), '1,2 M');
});

test('formatStats: con todas las partes', () => {
  const s = { calls: 5, failed: 1, durationMs: 45000, inputTokens: 45200, outputTokens: 3000, costUsd: 0.3125, requests: 5 };
  assert.equal(formatStats(s), '45 s · 48,2 k tokens · 0,31 $ · 5 peticiones · 1 fallida');
  const uno = { calls: 1, failed: 0, durationMs: 120000, inputTokens: 0, outputTokens: null, costUsd: 1, requests: 1 };
  assert.equal(formatStats(uno), '2 min · 0 tokens · 1,00 $ · 1 petición');
});

test('formatStats: solo con duración', () => {
  const s = { calls: 2, failed: 0, durationMs: 45000, inputTokens: null, outputTokens: null, costUsd: null, requests: null };
  assert.equal(formatStats(s), '45 s');
});

test('formatStats: con 0 llamadas devuelve cadena vacía', () => {
  assert.equal(formatStats(councilStats(null).total), '');
  assert.equal(formatStats({ calls: 0, failed: 0, durationMs: 0, inputTokens: null, outputTokens: null, costUsd: null, requests: null }), '');
});
