//! Detección de «cuenta sin cuota» en la salida de cada CLI (docs/plan-cuentas.md §5 y §8).

use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Exhaustion {
    /// Texto corto para el usuario, p. ej. «Límite semanal de Codex alcanzado».
    pub reason: String,
    /// Cuánto falta para que se reinicie, si la CLI lo dice.
    pub resets_in: Option<Duration>,
}

/// `cli`: id del agente (claude, codex, opencode, agy, gemini, copilot).
/// `output`: salida del proceso sin colores (final de agente.log). Solo cuentan las últimas 40 líneas
/// y solo si `exit_code != 0`.
pub fn detect(cli: &str, output: &str, exit_code: i32) -> Option<Exhaustion> {
    let _ = (cli, output, exit_code);
    None
}
