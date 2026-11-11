//! Salida en vivo de los paneles de la vista Agentes.
//!
//! `claude -p` con salida de texto no escribe nada hasta terminar: el panel se quedaba en blanco y al
//! final aparecía la respuesta de golpe. Los paneles de Claude se lanzan con
//! `--output-format stream-json --verbose` y aquí se traduce ese JSON (con el mismo intérprete que el
//! chat, `stream::Parser`) a texto de terminal según llega: lo que dice, las herramientas que usa y los
//! errores. Lo que no es JSON pasa tal cual.

use crate::stream::{Format, Parser, Step};
use std::collections::HashMap;
use std::path::Path;

const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

#[derive(Default)]
struct Shown {
    started: bool,
    ended: bool,
    text_len: usize,
}

pub struct SalidaViva {
    parser: Parser,
    shown: HashMap<String, Shown>,
    partial: String,
}

/// Una línea para el terminal (sin saltos dentro).
fn one_line(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s
    }
}

/// Saltos de línea de terminal.
fn crlf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\n', "\r\n")
}

impl SalidaViva {
    pub fn new(format: Format, cwd: &Path) -> Self {
        SalidaViva { parser: Parser::new(format, cwd), shown: HashMap::new(), partial: String::new() }
    }

    /// ¿Hay que lanzar esta CLI con salida en JSON para verla en vivo? Devuelve los argumentos a añadir.
    /// Solo Claude en modo `-p` y si quien la lanza no ha elegido ya un formato.
    pub fn extra_args(cli: &str, args: &[String]) -> Option<(Format, Vec<String>)> {
        let print = args.iter().any(|a| a == "-p" || a == "--print");
        let chosen = args.iter().any(|a| a == "--output-format" || a.starts_with("--output-format="));
        (cli == "claude" && print && !chosen).then(|| (Format::Claude, vec!["--output-format".into(), "stream-json".into(), "--verbose".into()]))
    }

    /// Recibe un trozo de la salida; devuelve lo que hay que pintar. `None`: fin de la salida.
    pub fn push(&mut self, data: Option<&str>) -> String {
        let mut out = String::new();
        match data {
            Some(d) => {
                self.partial.push_str(d);
                while let Some(pos) = self.partial.find('\n') {
                    let line: String = self.partial.drain(..=pos).collect();
                    self.line(line.trim_end_matches(['\n', '\r']), &mut out);
                }
            }
            None => {
                let rest = std::mem::take(&mut self.partial);
                if !rest.trim().is_empty() {
                    self.line(rest.trim_end(), &mut out);
                }
            }
        }
        out
    }

    fn line(&mut self, line: &str, out: &mut String) {
        if line.trim().is_empty() {
            return;
        }
        if !line.trim_start().starts_with('{') {
            // Avisos o errores de la propia CLI, fuera del JSON.
            out.push_str(&crlf(line));
            out.push_str("\r\n");
            return;
        }
        for step in self.parser.feed(line) {
            self.render(&step, out);
        }
    }

    fn render(&mut self, s: &Step, out: &mut String) {
        let shown = self.shown.entry(s.id.clone()).or_default();
        match s.kind.as_str() {
            "text" => {
                if s.detail.len() > shown.text_len && s.detail.is_char_boundary(shown.text_len) {
                    let new = &s.detail[shown.text_len..];
                    if shown.text_len == 0 {
                        out.push_str("\r\n");
                    }
                    out.push_str(&crlf(new));
                    shown.text_len = s.detail.len();
                    if s.status != "running" {
                        out.push_str("\r\n");
                    }
                }
            }
            "thinking" => {
                if !shown.started {
                    shown.started = true;
                    out.push_str(&format!("{DIM}· {}{RESET}\r\n", one_line(&s.detail, 160)));
                }
            }
            "error" => {
                if !shown.ended {
                    shown.ended = true;
                    out.push_str(&format!("{RED}✗ {}{RESET}\r\n", crlf(&s.detail)));
                }
            }
            _ => {
                if !shown.started {
                    shown.started = true;
                    let detail = if s.detail.is_empty() || s.detail == s.title { String::new() } else { format!(" {}", one_line(&s.detail, 200)) };
                    out.push_str(&format!("{CYAN}▸ {}{RESET}{DIM}{detail}{RESET}\r\n", one_line(&s.title, 60)));
                }
                if !shown.ended && s.status == "error" {
                    shown.ended = true;
                    let tail: Vec<&str> = s.output.as_deref().unwrap_or("").lines().filter(|l| !l.trim().is_empty()).collect();
                    let tail = tail[tail.len().saturating_sub(4)..].join("\r\n    ");
                    let code = s.exit_code.map(|c| format!(" (código {c})")).unwrap_or_default();
                    out.push_str(&format!("{RED}  ✗ falló{code}{RESET}"));
                    if !tail.is_empty() {
                        out.push_str(&format!("\r\n{DIM}    {tail}{RESET}"));
                    }
                    out.push_str("\r\n");
                } else if !shown.ended && s.status == "done" {
                    shown.ended = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_claude_en_modo_print() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(SalidaViva::extra_args("claude", &a(&["-p", "--permission-mode", "acceptEdits"])).is_some());
        assert!(SalidaViva::extra_args("claude", &a(&["-p", "--output-format", "json"])).is_none());
        assert!(SalidaViva::extra_args("claude", &a(&["--output-format=text", "-p"])).is_none());
        assert!(SalidaViva::extra_args("claude", &a(&[])).is_none());
        assert!(SalidaViva::extra_args("codex", &a(&["exec", "-p"])).is_none());
    }

    #[test]
    fn traduce_el_json_de_claude_segun_llega() {
        let mut v = SalidaViva::new(Format::Claude, Path::new("/repo"));
        // Una línea partida entre dos lecturas no se pinta hasta que llega entera.
        let a = r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"Voy a mirar"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls /repo/src","description":"Listar"}}]}}"#;
        assert_eq!(v.push(Some(&a[..30])), "");
        let out = v.push(Some(&format!("{}\n", &a[30..])));
        assert!(out.contains("Voy a mirar"));
        assert!(out.contains("▸ Listar"));
        assert!(out.contains("ls /repo/src"));
        // Resultado bien: no repite nada.
        let ok = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"a\nb","is_error":false}]}}"#;
        assert_eq!(v.push(Some(&format!("{ok}\n"))), "");
        // Resultado con error: lo dice con las últimas líneas.
        let b = r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"mvn test"}}]}}"#;
        let err = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"BUILD FAILURE","is_error":true}]}}"#;
        let out = v.push(Some(&format!("{b}\n{err}\n")));
        assert!(out.contains("mvn test") && out.contains("falló") && out.contains("BUILD FAILURE"));
        // Lo que no es JSON pasa tal cual; al final se vacía lo pendiente.
        assert!(v.push(Some("Error: algo raro\n")).contains("Error: algo raro"));
        let fin = r#"{"type":"result","is_error":true,"result":"Claude AI usage limit reached"}"#;
        assert!(v.push(Some(fin)).is_empty());
        assert!(v.push(None).contains("usage limit reached"));
    }
}
