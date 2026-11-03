//! Detección de «cuenta sin cuota» en la salida de cada CLI (docs/plan-cuentas.md §5 y §8).
//!
//! Solo se miran las últimas líneas y solo si el proceso falló: un agente que escribe código que
//! menciona «rate limit» y termina bien (o falla por otra cosa muchas líneas después) no es un
//! agotamiento. Los textos salen de errores reales de cada CLI (ver los tests).

use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Exhaustion {
    /// Texto corto para el usuario, p. ej. «Límite semanal de Codex alcanzado».
    pub reason: String,
    /// Cuánto falta para que se reinicie, si la CLI lo dice.
    pub resets_in: Option<Duration>,
}

const TAIL_LINES: usize = 40;

/// Frases de agotamiento por CLI (en minúsculas). Las comunes valen para todas.
fn markers(cli: &str) -> &'static [&'static str] {
    match cli {
        // Binario de Claude Code: «You've hit your limit», «Usage limit reached», «out of usage credits»…
        "claude" => &["hit your limit", "hit your monthly", "usage limit reached", "out of usage", "out of extra usage", "credit balance is too low", "claude ai usage limit"],
        // ~/.codex/sessions: «You've hit your usage limit. Upgrade to Pro … or try again at 6:11 AM.»
        "codex" => &["hit your usage limit", "usage_limit_reached", "usage_limit_exceeded"],
        // «OpenCode Go monthly usage limit reached…», «Upstream request failed: Insufficient account funds»
        "opencode" => &["usage limit reached", "insufficient account funds", "insufficient funds"],
        // «error: Individual quota reached. … Resets in 23h21m49s.» + AGY_ERROR … RESOURCE_EXHAUSTED (code 429)
        "agy" => &["individual quota reached", "quota reached", "resource_exhausted"],
        // Genéricos conservadores (sin ejemplo real en esta máquina).
        "gemini" => &["resource_exhausted", "quota exceeded", "exhausted your", "daily quota"],
        "copilot" => &["quota exceeded", "exceeded your copilot", "premium request", "monthly limit"],
        _ => &[],
    }
}

/// Frases que parecen agotamiento pero no lo son.
const NOT_QUOTA: &[&str] = &[
    "trains on request data",   // aviso de privacidad de OpenCode Go
    "context limit reached",    // ventana de contexto, no cuota
    "context window",
    "recursion limit reached",
    "nesting limit reached",
];

fn name(cli: &str) -> &str {
    match cli {
        "claude" => "Claude",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "agy" => "Antigravity",
        "gemini" => "Gemini",
        "copilot" => "Copilot",
        other => other,
    }
}

/// `cli`: id del agente (claude, codex, opencode, agy, gemini, copilot).
/// `output`: salida del proceso sin colores (final de agente.log). Solo cuentan las últimas 40 líneas
/// y solo si `exit_code != 0`.
pub fn detect(cli: &str, output: &str, exit_code: i32) -> Option<Exhaustion> {
    if exit_code == 0 {
        return None;
    }
    let lines: Vec<&str> = output.lines().collect();
    let tail = &lines[lines.len().saturating_sub(TAIL_LINES)..];
    let marks = markers(cli);
    for line in tail.iter().rev() {
        let l = line.to_lowercase();
        if NOT_QUOTA.iter().any(|n| l.contains(n)) {
            continue;
        }
        if marks.iter().any(|m| l.contains(m)) {
            let resets_in = tail.iter().find_map(|t| resets_in(t));
            let mut reason = format!("Cuota de {} agotada", name(cli));
            if let Some(d) = resets_in {
                reason.push_str(&format!(" (se reinicia en {})", human(d)));
            }
            return Some(Exhaustion { reason, resets_in });
        }
    }
    None
}

/// «Resets in 23h21m49s», «resets in 4h 20m», «Resets in 45m».
fn resets_in(line: &str) -> Option<Duration> {
    let l = line.to_lowercase();
    let at = l.find("resets in ")? + "resets in ".len();
    let mut secs = 0u64;
    let mut num = String::new();
    let mut any = false;
    for c in l[at..].chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        if c == ' ' && num.is_empty() {
            continue;
        }
        let mult = match c {
            'd' => 86_400,
            'h' => 3_600,
            'm' => 60,
            's' => 1,
            ' ' => continue,
            _ => break,
        };
        if num.is_empty() {
            break;
        }
        secs += num.parse::<u64>().ok()? * mult;
        num.clear();
        any = true;
    }
    (any && secs > 0).then(|| Duration::from_secs(secs))
}

fn human(d: Duration) -> String {
    let m = d.as_secs() / 60;
    let (days, h, mm) = (m / 1440, (m % 1440) / 60, m % 60);
    if days > 0 {
        format!("{days} d {h} h")
    } else if h > 0 {
        format!("{h} h {mm} min")
    } else {
        format!("{mm} min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGY: &str = "Cargando…\nerror: Individual quota reached. Please upgrade your subscription to increase your limits. Resets in 23h21m49s.\nAGY_ERROR: {\"short_error\":\"RESOURCE_EXHAUSTED (code 429): Individual quota reached. Please upgrade your subscription to increase your limits. Resets in 23h21m49s.\",\"status\":\"RESOURCE_EXHAUSTED\",\"error_code\":429,\"code_kind\":\"http\",\"retryable\":true}";
    const OC_MONTHLY: &str = "> build · kimi-k2.7-code\nError: OpenCode Go monthly usage limit reached. Switch to a free model, or turn on Extra Usage on the Go page in OpenCode Console to keep going.";
    const OC_FUNDS: &str = "Error: Upstream request failed: Insufficient account funds";
    const OC_PRIVACY: &str = "Error: Upstream request failed: This Go model trains on request data. Allow paid endpoints that train on request data in your workspace's Privacy settings to use it.";
    // ~/.codex/sessions (02/11/2026)
    const CODEX: &str = "exec\ncargo test\nERROR: You've hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at 6:11 AM.";
    const CODEX_PLUS: &str = "You've hit your usage limit. Upgrade to Plus to continue using Codex (https://chatgpt.com/explore/plus), or try again at Nov 8th, 2026 1:45 PM.";
    // Textos del binario de Claude Code 2.1.316
    const CLAUDE: &str = "You've hit your limit · resets 3am (Europe/Madrid)";
    const CLAUDE_CREDITS: &str = "API Error: You're out of usage credits. Contact your admin to continue.";

    #[test]
    fn mensajes_reales() {
        let a = detect("agy", AGY, 1).unwrap();
        assert_eq!(a.resets_in, Some(Duration::from_secs(23 * 3600 + 21 * 60 + 49)));
        assert_eq!(a.reason, "Cuota de Antigravity agotada (se reinicia en 23 h 21 min)");
        assert!(detect("opencode", OC_MONTHLY, 1).is_some());
        assert!(detect("opencode", OC_FUNDS, 1).is_some());
        assert!(detect("codex", CODEX, 1).is_some());
        assert!(detect("codex", CODEX_PLUS, 1).is_some());
        assert!(detect("claude", CLAUDE, 1).is_some());
        assert!(detect("claude", CLAUDE_CREDITS, 1).is_some());
        assert!(detect("gemini", "Error: [429] RESOURCE_EXHAUSTED: Quota exceeded for quota metric", 1).is_some());
    }

    #[test]
    fn falsos_positivos() {
        // Aviso de privacidad de OpenCode Go
        assert!(detect("opencode", OC_PRIVACY, 1).is_none());
        // Terminó bien aunque el texto lo diga
        assert!(detect("codex", CODEX, 0).is_none());
        // Error de compilación
        assert!(detect("codex", "error[E0599]: no method named `foo`\nerror: could not compile `x`", 101).is_none());
        // Escribió código con «usage limit» hace más de 40 líneas y luego falló un test
        let mut out = String::from("fn check() { /* You've hit your usage limit */ }\n");
        for i in 0..60 {
            out.push_str(&format!("test caso_{i} ... ok\n"));
        }
        out.push_str("test result: FAILED. 1 failed");
        assert!(detect("codex", &out, 1).is_none());
        // 429 de un servidor de prueba local
        assert!(detect("claude", "curl: (22) The requested URL returned error: 429\nhttp://localhost:3000/api", 22).is_none());
        // Ventana de contexto, no cuota
        assert!(detect("claude", "Context limit reached · /compact or /clear to continue", 1).is_none());
        // CLI desconocida
        assert!(detect("bash", "usage limit reached", 1).is_none());
    }

    #[test]
    fn plazos() {
        assert_eq!(resets_in("Resets in 4h 20m"), Some(Duration::from_secs(4 * 3600 + 20 * 60)));
        assert_eq!(resets_in("resets in 45m."), Some(Duration::from_secs(45 * 60)));
        assert_eq!(resets_in("Resets in 2d3h"), Some(Duration::from_secs(2 * 86400 + 3 * 3600)));
        assert_eq!(resets_in("no dice nada"), None);
    }
}
