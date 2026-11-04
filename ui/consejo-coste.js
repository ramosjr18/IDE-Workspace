(function (root) {
  // Plantilla de estadística vacía: llamadas y duración a 0, el resto null.
  const emptyStat = () => ({ calls: 0, failed: 0, durationMs: 0, inputTokens: null, outputTokens: null, costUsd: null, requests: null });

  // Acumula una llamada en una estadística (usage: solo los campos que existan; si ninguno, quedan null).
  const addCall = (s, call) => {
    if (!call) return s;
    s.calls++;
    if (call.ok === false) s.failed++;
    s.durationMs += call.durationMs || 0;
    const u = call.usage;
    for (const f of ['inputTokens', 'outputTokens', 'costUsd', 'requests']) {
      if (u && u[f] != null) s[f] = (s[f] == null ? 0 : s[f]) + u[f];
    }
    return s;
  };

  // Agrega las llamadas del consejo: { total, byAgent } con una estadística por agente en orden de primera aparición.
  const councilStats = (council) => {
    const calls = council && Array.isArray(council.calls) ? council.calls : [];
    const total = emptyStat();
    const byAgent = [];
    const seen = new Map();
    for (const call of calls) {
      let s = seen.get(call.agent);
      if (!s) {
        s = { agent: call.agent, agentName: call.agentName, ...emptyStat() };
        seen.set(call.agent, s);
        byAgent.push(s);
      }
      if (!s.agentName && call.agentName) s.agentName = call.agentName;
      addCall(s, call);
      addCall(total, call);
    }
    return { total, byAgent };
  };

  // Formatea ms como «N s» (<60 s), «M min [S s]» (<1 h) o «H h M min».
  const formatDuration = (ms) => {
    const s = Math.round(ms / 1000);
    if (s < 60) return `${s} s`;
    if (s < 3600) {
      const m = Math.floor(s / 60), r = s % 60;
      return r ? `${m} min ${r} s` : `${m} min`;
    }
    return `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min`;
  };

  // Formatea tokens: tal cual (<1000), o miles/millones con un decimal y coma («45,2 k», «3 k», «1,2 M»).
  const formatTokens = (n) => {
    if (n < 1000) return String(n);
    const div = n < 1000000 ? 1000 : 1000000;
    const suf = n < 1000000 ? ' k' : ' M';
    const t = Math.round((n / div) * 10);
    const e = Math.floor(t / 10), d = t % 10;
    return `${e}${d ? ',' + d : ''}${suf}`;
  };

  // Formatea una estadística como cadena con las partes que haya unidas por « · » ('' con 0 llamadas).
  const formatStats = (s) => {
    if (!s || s.calls === 0) return '';
    const parts = [];
    if (s.calls > 0) parts.push(formatDuration(s.durationMs));
    if (s.inputTokens !== null || s.outputTokens !== null) parts.push(`${formatTokens((s.inputTokens || 0) + (s.outputTokens || 0))} tokens`);
    if (s.costUsd !== null) parts.push(`${(Math.round(s.costUsd * 100) / 100).toFixed(2).replace('.', ',')} $`);
    if (s.requests !== null) parts.push(`${s.requests} ${s.requests === 1 ? 'petición' : 'peticiones'}`);
    if (s.failed > 0) parts.push(`${s.failed} ${s.failed === 1 ? 'fallida' : 'fallidas'}`);
    return parts.join(' · ');
  };

  const api = { councilStats, formatDuration, formatTokens, formatStats };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.ConsejoCoste = api;
})(typeof window !== "undefined" ? window : globalThis);
