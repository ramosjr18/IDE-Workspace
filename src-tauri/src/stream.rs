//! Traduce la salida estructurada (JSON por líneas) de cada CLI a pasos comunes que el chat
//! muestra en vivo: comandos, archivos leídos/editados, búsquedas, tareas, texto…

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Formato de salida de cada CLI.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Format {
    /// `claude -p --output-format stream-json --verbose`
    Claude,
    /// `codex exec --json`
    Codex,
    /// `opencode run --format json`
    OpenCode,
    /// `agy --output-format stream-json`
    Agy,
    /// `copilot --output-format json`
    Copilot,
    /// Texto sin estructura: toda la salida es la respuesta.
    Plain,
}

/// Un paso visible en el chat. Se identifica por `id` y se actualiza mientras avanza.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    /// text | thinking | command | edit | read | search | web | todo | agent | tool | error
    pub kind: String,
    /// Nombre de la herramienta o descripción breve.
    #[serde(default)]
    pub title: String,
    /// Comando, ruta, patrón… o el texto en los pasos de texto.
    #[serde(default)]
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// running | done | error
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// Uso que informa la CLI: tokens, coste y peticiones facturadas.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// Tokens de entrada (con las cachés leídas/creadas).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub input_tokens: Option<u64>,
    /// Tokens de salida (con la razón).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub output_tokens: Option<u64>,
    /// Coste acumulado en dólares.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub cost_usd: Option<f64>,
    /// Peticiones facturadas aparte (Copilot premium).
    #[serde(default, skip_serializing_if = "Option::is_none")] pub requests: Option<f64>,
}

impl Usage {
    /// Suma otro uso en este, campo a campo: None + None es None y None cuenta como 0.
    pub fn add(&mut self, other: &Usage) {
        fn sumar_u64(a: &mut Option<u64>, b: Option<u64>) {
            *a = match (a.take(), b) {
                (None, b) => b,
                (a, None) => a,
                (Some(x), Some(y)) => Some(x + y),
            };
        }
        fn sumar_f64(a: &mut Option<f64>, b: Option<f64>) {
            *a = match (a.take(), b) {
                (None, b) => b,
                (a, None) => a,
                (Some(x), Some(y)) => Some(x + y),
            };
        }
        sumar_u64(&mut self.input_tokens, other.input_tokens);
        sumar_u64(&mut self.output_tokens, other.output_tokens);
        sumar_f64(&mut self.cost_usd, other.cost_usd);
        sumar_f64(&mut self.requests, other.requests);
    }
}

const MAX_OUTPUT: usize = 8000;

fn tail(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let skip = count - max;
    format!("…{}", s.chars().skip(skip).collect::<String>())
}

/// Tipo de paso según el nombre de la herramienta.
fn classify(name: &str) -> &'static str {
    let n = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if has(&["todo", "plan"]) {
        "todo"
    } else if has(&["web", "fetch", "url", "browser"]) {
        "web"
    } else if has(&["bash", "shell", "command", "exec", "terminal"]) {
        "command"
    } else if has(&["edit", "write", "patch", "replace", "create", "notebook", "multiedit", "delete"]) {
        "edit"
    } else if has(&["read", "view", "cat", "open"]) {
        "read"
    } else if has(&["grep", "glob", "search", "find", "list", "ls"]) {
        "search"
    } else if has(&["task", "agent", "subagent"]) {
        "agent"
    } else {
        "tool"
    }
}

fn str_field<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v.get(*k).and_then(Value::as_str)).filter(|s| !s.is_empty())
}

/// Primer número entero que exista entre las claves, o 0 si falta.
fn u64_field(v: &Value, keys: &[&str]) -> u64 {
    keys.iter().find_map(|k| v.get(*k).and_then(Value::as_u64)).unwrap_or(0)
}

/// Texto de un resultado que puede ser cadena o lista de bloques {type:text, text}.
fn content_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str).or_else(|| i.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Quita la envoltura `/bin/zsh -lc "…"` que Codex pone a los comandos.
fn unwrap_shell(cmd: &str) -> String {
    for sh in ["zsh", "bash", "sh"] {
        for flag in [" -lc ", " -c "] {
            if let Some(pos) = cmd.find(&format!("{sh}{flag}")) {
                if cmd[..pos].chars().all(|c| c != ' ') {
                    let rest = cmd[pos + sh.len() + flag.len()..].trim();
                    let unq = rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or(rest);
                    return unq.replace("\\\"", "\"");
                }
            }
        }
    }
    cmd.to_string()
}

pub struct Parser {
    format: Format,
    cwd: String,
    cwd_alt: String,
    steps: Vec<Step>,
    index: HashMap<String, usize>,
    final_text: Option<String>,
    plain: String,
    usage: Usage,
    session_id: Option<String>,
}

impl Parser {
    pub fn new(format: Format, cwd: &Path) -> Self {
        let cwd = cwd.to_string_lossy().trim_end_matches('/').to_string();
        // En macOS /tmp es /private/tmp: algunas CLIs informan una u otra ruta.
        let cwd_alt = cwd.strip_prefix("/private").map(str::to_string).unwrap_or_else(|| format!("/private{cwd}"));
        Parser { format, cwd, cwd_alt, steps: vec![], index: HashMap::new(), final_text: None, plain: String::new(), usage: Usage::default(), session_id: None }
    }

    /// Ruta relativa al proyecto si está dentro de él.
    fn rel(&self, p: &str) -> String {
        for base in [&self.cwd, &self.cwd_alt] {
            if let Some(r) = p.strip_prefix(base.as_str()) {
                let r = r.trim_start_matches(['/', '\\']);
                if !r.is_empty() {
                    return r.to_string();
                }
            }
        }
        p.to_string()
    }

    /// Detalle de una llamada a herramienta: comando, ruta, patrón, URL…
    fn detail_of(&self, input: &Value) -> String {
        if let Some(s) = input.as_str() {
            // apply_patch de Copilot/Codex: se extraen las rutas del parche.
            let files: Vec<String> = s
                .lines()
                .filter_map(|l| {
                    ["*** Update File: ", "*** Add File: ", "*** Delete File: "].iter().find_map(|p| l.strip_prefix(p))
                })
                .map(|p| self.rel(p.trim()))
                .collect();
            return if files.is_empty() { tail(s, 200) } else { files.join(", ") };
        }
        if let Some(c) = str_field(input, &["command", "CommandLine", "cmd"]) {
            return c.to_string();
        }
        if let Some(p) = str_field(input, &["file_path", "filePath", "path", "Path", "TargetFile", "AbsolutePath", "notebook_path"]) {
            return self.rel(p);
        }
        if let Some(q) = str_field(input, &["pattern", "query", "Query", "SearchPath", "url", "Url", "description", "prompt"]) {
            return q.to_string();
        }
        if let Some(todos) = input.get("todos").and_then(Value::as_array) {
            return todos
                .iter()
                .filter_map(|t| {
                    let text = str_field(t, &["content", "text", "title"])?;
                    let done = str_field(t, &["status"]).is_some_and(|s| s == "completed");
                    Some(format!("{} {text}", if done { "☑" } else { "☐" }))
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
        String::new()
    }

    /// Crea o actualiza un paso; devuelve su estado actual.
    fn put(&mut self, step: Step) -> Step {
        if let Some(&i) = self.index.get(&step.id) {
            let cur = &mut self.steps[i];
            if !step.kind.is_empty() {
                cur.kind = step.kind;
            }
            if !step.title.is_empty() {
                cur.title = step.title;
            }
            if !step.detail.is_empty() {
                cur.detail = step.detail;
            }
            if step.output.is_some() {
                cur.output = step.output;
            }
            if !step.status.is_empty() {
                cur.status = step.status;
            }
            if step.exit_code.is_some() {
                cur.exit_code = step.exit_code;
            }
            return cur.clone();
        }
        self.index.insert(step.id.clone(), self.steps.len());
        self.steps.push(step.clone());
        step
    }

    fn append_text(&mut self, id: &str, delta: &str, done: bool) -> Step {
        let status = if done { "done" } else { "running" };
        if let Some(&i) = self.index.get(id) {
            let cur = &mut self.steps[i];
            cur.detail.push_str(delta);
            cur.status = status.into();
            return cur.clone();
        }
        self.put(Step { id: id.into(), kind: "text".into(), detail: delta.into(), status: status.into(), ..Default::default() })
    }

    fn tool_start(&mut self, id: &str, name: &str, input: &Value, title: Option<&str>) -> Step {
        let kind = classify(name);
        let mut detail = self.detail_of(input);
        if kind == "command" {
            detail = unwrap_shell(&detail);
        }
        let title = title.map(str::to_string).or_else(|| str_field(input, &["description"]).map(str::to_string)).unwrap_or_else(|| name.to_string());
        self.put(Step { id: id.into(), kind: kind.into(), title, detail, status: "running".into(), ..Default::default() })
    }

    fn tool_end(&mut self, id: &str, output: String, ok: bool, exit_code: Option<i32>) -> Step {
        let output = if output.trim().is_empty() { None } else { Some(tail(&output, MAX_OUTPUT)) };
        self.put(Step { id: id.into(), output, status: if ok { "done" } else { "error" }.into(), exit_code, ..Default::default() })
    }

    /// Procesa una línea de la salida. Devuelve los pasos que han cambiado.
    pub fn feed(&mut self, line: &str) -> Vec<Step> {
        if self.format == Format::Plain {
            self.plain.push_str(line);
            self.plain.push('\n');
            let text = self.plain.trim_end().to_string();
            return vec![self.put(Step { id: "out".into(), kind: "text".into(), detail: text, status: "running".into(), ..Default::default() })];
        }
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return vec![] };
        if let Some(id) = session_of(self.format, &v) {
            self.session_id = Some(id);
        }
        match self.format {
            Format::Claude => self.feed_claude(&v),
            Format::Codex => self.feed_codex(&v),
            Format::OpenCode => self.feed_opencode(&v),
            Format::Agy => self.feed_agy(&v),
            Format::Copilot => self.feed_copilot(&v),
            Format::Plain => vec![],
        }
    }

    fn feed_claude(&mut self, v: &Value) -> Vec<Step> {
        let mut out = vec![];
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                let msg = &v["message"];
                let msg_id = msg.get("id").and_then(Value::as_str).unwrap_or("m");
                for (i, block) in msg["content"].as_array().into_iter().flatten().enumerate() {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let text = block["text"].as_str().unwrap_or("");
                            if !text.trim().is_empty() {
                                out.push(self.put(Step {
                                    id: format!("{msg_id}-{i}"),
                                    kind: "text".into(),
                                    detail: text.into(),
                                    status: "done".into(),
                                    ..Default::default()
                                }));
                            }
                        }
                        Some("thinking") => {
                            let text = block["thinking"].as_str().unwrap_or("");
                            if !text.trim().is_empty() {
                                out.push(self.put(Step {
                                    id: format!("{msg_id}-{i}"),
                                    kind: "thinking".into(),
                                    detail: text.into(),
                                    status: "done".into(),
                                    ..Default::default()
                                }));
                            }
                        }
                        Some("tool_use") => {
                            let id = block["id"].as_str().unwrap_or("t").to_string();
                            let name = block["name"].as_str().unwrap_or("tool");
                            out.push(self.tool_start(&id, name, &block["input"], None));
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                for block in v["message"]["content"].as_array().into_iter().flatten() {
                    if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                        let id = block["tool_use_id"].as_str().unwrap_or("");
                        let ok = !block["is_error"].as_bool().unwrap_or(false);
                        out.push(self.tool_end(id, content_text(&block["content"]), ok, None));
                    }
                }
            }
            Some("result") => {
                // Uso y coste del resultado: las cachés creadas/leídas ya son tokens de entrada.
                let u = &v["usage"];
                if u.is_object() {
                    self.usage.add(&Usage {
                        input_tokens: Some(
                            u["input_tokens"].as_u64().unwrap_or(0)
                                + u["cache_creation_input_tokens"].as_u64().unwrap_or(0)
                                + u["cache_read_input_tokens"].as_u64().unwrap_or(0),
                        ),
                        output_tokens: Some(u["output_tokens"].as_u64().unwrap_or(0)),
                        cost_usd: v["total_cost_usd"].as_f64(),
                        requests: None,
                    });
                }
                if let Some(r) = v.get("result").and_then(Value::as_str) {
                    self.final_text = Some(r.to_string());
                }
                if v["is_error"].as_bool().unwrap_or(false) {
                    out.push(self.put(Step {
                        id: "result-error".into(),
                        kind: "error".into(),
                        detail: v.get("result").and_then(Value::as_str).unwrap_or("Error").into(),
                        status: "error".into(),
                        ..Default::default()
                    }));
                }
            }
            _ => {}
        }
        out
    }

    fn feed_codex(&mut self, v: &Value) -> Vec<Step> {
        let ty = v.get("type").and_then(Value::as_str).unwrap_or("");
        if ty == "turn.completed" {
            // Uso del turno (los tokens de entrada ya incluyen los de caché).
            let u = &v["usage"];
            self.usage.add(&Usage {
                input_tokens: Some(u["input_tokens"].as_u64().unwrap_or(0)),
                output_tokens: Some(u["output_tokens"].as_u64().unwrap_or(0)),
                cost_usd: None,
                requests: None,
            });
            return vec![];
        }
        if ty == "error" || ty == "turn.failed" {
            let msg = str_field(v, &["message"]).map(str::to_string).or_else(|| v["error"]["message"].as_str().map(str::to_string)).unwrap_or_default();
            return vec![self.put(Step { id: format!("error-{}", self.steps.len()), kind: "error".into(), detail: msg, status: "error".into(), ..Default::default() })];
        }
        if !ty.starts_with("item.") {
            return vec![];
        }
        let done = ty == "item.completed";
        let item = &v["item"];
        let id = item["id"].as_str().unwrap_or("i").to_string();
        let status = match item["status"].as_str() {
            Some("failed") => "error",
            _ if done => "done",
            _ => "running",
        };
        let step = match item["type"].as_str().unwrap_or("") {
            "agent_message" => {
                let text = item["text"].as_str().unwrap_or("").to_string();
                if done {
                    self.final_text = Some(text.clone());
                }
                Step { id, kind: "text".into(), detail: text, status: status.into(), ..Default::default() }
            }
            "reasoning" => Step { id, kind: "thinking".into(), detail: item["text"].as_str().unwrap_or("").into(), status: status.into(), ..Default::default() },
            "command_execution" => {
                let exit = item["exit_code"].as_i64().map(|c| c as i32);
                let output = item["aggregated_output"].as_str().filter(|s| !s.is_empty()).map(|s| tail(s, MAX_OUTPUT));
                let status = if exit.is_some_and(|c| c != 0) { "error" } else { status };
                Step {
                    id,
                    kind: "command".into(),
                    title: "Comando".into(),
                    detail: unwrap_shell(item["command"].as_str().unwrap_or("")),
                    output,
                    status: status.into(),
                    exit_code: exit,
                }
            }
            "file_change" => {
                let files: Vec<String> = item["changes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["path"].as_str().map(|p| self.rel(p)))
                    .collect();
                Step { id, kind: "edit".into(), title: "Cambio de archivos".into(), detail: files.join(", "), status: status.into(), ..Default::default() }
            }
            "mcp_tool_call" => Step {
                id,
                kind: "tool".into(),
                title: format!("{}·{}", item["server"].as_str().unwrap_or("mcp"), item["tool"].as_str().unwrap_or("")),
                detail: String::new(),
                status: status.into(),
                ..Default::default()
            },
            "web_search" => Step { id, kind: "web".into(), title: "Búsqueda web".into(), detail: item["query"].as_str().unwrap_or("").into(), status: status.into(), ..Default::default() },
            "todo_list" => {
                let detail = item["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|t| format!("{} {}", if t["completed"].as_bool().unwrap_or(false) { "☑" } else { "☐" }, t["text"].as_str().unwrap_or("")))
                    .collect::<Vec<_>>()
                    .join("\n");
                Step { id, kind: "todo".into(), title: "Tareas".into(), detail, status: status.into(), ..Default::default() }
            }
            "error" => Step { id, kind: "error".into(), detail: item["message"].as_str().unwrap_or("").into(), status: "error".into(), ..Default::default() },
            _ => return vec![],
        };
        vec![self.put(step)]
    }

    fn feed_opencode(&mut self, v: &Value) -> Vec<Step> {
        let part = &v["part"];
        match v.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = part["text"].as_str().unwrap_or("").to_string();
                if text.trim().is_empty() {
                    return vec![];
                }
                self.final_text = Some(text.clone());
                let id = part["id"].as_str().unwrap_or("t").to_string();
                vec![self.put(Step { id, kind: "text".into(), detail: text, status: "done".into(), ..Default::default() })]
            }
            Some("reasoning") => {
                let id = part["id"].as_str().unwrap_or("r").to_string();
                vec![self.put(Step { id, kind: "thinking".into(), detail: part["text"].as_str().unwrap_or("").into(), status: "done".into(), ..Default::default() })]
            }
            Some("tool_use") => {
                let id = part["callID"].as_str().or(part["id"].as_str()).unwrap_or("tool").to_string();
                let name = part["tool"].as_str().unwrap_or("tool");
                let state = &part["state"];
                let mut steps = vec![self.tool_start(&id, name, &state["input"], state["title"].as_str().filter(|_| classify(name) != "command"))];
                match state["status"].as_str() {
                    Some("completed") | Some("error") => {
                        let ok = state["status"].as_str() == Some("completed");
                        let exit = state["metadata"]["exit"].as_i64().map(|c| c as i32);
                        let output = state["output"].as_str().or(state["error"].as_str()).unwrap_or("").to_string();
                        let ok = ok && exit.is_none_or(|c| c == 0);
                        steps.push(self.tool_end(&id, output, ok, exit));
                    }
                    _ => {}
                }
                steps
            }
            Some("error") => {
                let msg = v["error"]["data"]["message"].as_str().or(v["error"]["message"].as_str()).unwrap_or("Error").to_string();
                vec![self.put(Step { id: format!("error-{}", self.steps.len()), kind: "error".into(), detail: msg, status: "error".into(), ..Default::default() })]
            }
            Some("step_finish") => {
                // Uso del paso: las cachés cuentan como entrada y la razón como salida.
                let t = &part["tokens"];
                self.usage.add(&Usage {
                    input_tokens: Some(
                        t["input"].as_u64().unwrap_or(0)
                            + t["cache"]["read"].as_u64().unwrap_or(0)
                            + t["cache"]["write"].as_u64().unwrap_or(0),
                    ),
                    output_tokens: Some(t["output"].as_u64().unwrap_or(0) + t["reasoning"].as_u64().unwrap_or(0)),
                    cost_usd: part["cost"].as_f64(),
                    requests: None,
                });
                vec![]
            }
            _ => vec![],
        }
    }

    fn feed_agy(&mut self, v: &Value) -> Vec<Step> {
        match v.get("event").and_then(Value::as_str) {
            Some("step_update") => {
                let s = &v["step_update"];
                let idx = s["step_index"].as_i64().unwrap_or(0);
                let id = format!("agy-{idx}");
                let state = s["state"].as_str().unwrap_or("");
                let status = match state {
                    "DONE" => "done",
                    "ERROR" | "FAILED" | "CANCELED" => "error",
                    _ => "running",
                };
                match s["step_type"].as_str() {
                    Some("agent_response") => match s["text_delta"].as_str() {
                        Some(delta) => vec![self.append_text(&id, delta, status == "done")],
                        None if status == "done" && self.index.contains_key(&id) => vec![self.append_text(&id, "", true)],
                        None => vec![],
                    },
                    Some("tool") => {
                        let info = &s["tool_info"];
                        let name = s["tool_name"].as_str().or(info["name"].as_str()).unwrap_or("tool");
                        let mut steps = vec![self.tool_start(&id, name, &info["parameters"], None)];
                        if status != "running" {
                            let output = info["output"].as_str().unwrap_or("").to_string();
                            let exit = output
                                .strip_prefix("Command exited with code ")
                                .and_then(|r| r.split('.').next())
                                .and_then(|n| n.trim().parse::<i32>().ok());
                            let output = output.split_once("Command output:\n").map(|(_, o)| o.to_string()).unwrap_or(output);
                            let ok = status == "done" && exit.is_none_or(|c| c == 0);
                            steps.push(self.tool_end(&id, output.replace("\r\n", "\n"), ok, exit));
                        }
                        steps
                    }
                    _ => vec![],
                }
            }
            Some("result") => {
                // Uso de tokens en el resultado (la ruta exacta varía según la versión de agy).
                let u = if v["usage"].is_object() { &v["usage"] } else { &v["result"]["usage"] };
                if u.is_object() {
                    self.usage.add(&Usage {
                        input_tokens: Some(u64_field(u, &["input_tokens", "inputTokens", "prompt_tokens"])),
                        output_tokens: Some(u64_field(u, &["output_tokens", "outputTokens", "completion_tokens"])),
                        cost_usd: None,
                        requests: None,
                    });
                }
                let r = &v["result"];
                if let Some(text) = r["response"].as_str() {
                    self.final_text = Some(text.trim().to_string());
                }
                if r["status"].as_str().is_some_and(|s| s != "SUCCESS") {
                    let msg = r["error"].as_str().or(r["error"]["message"].as_str()).unwrap_or("Antigravity terminó con error").to_string();
                    return vec![self.put(Step { id: "agy-error".into(), kind: "error".into(), detail: msg, status: "error".into(), ..Default::default() })];
                }
                vec![]
            }
            _ => vec![],
        }
    }

    fn feed_copilot(&mut self, v: &Value) -> Vec<Step> {
        let d = &v["data"];
        match v.get("type").and_then(Value::as_str) {
            Some("assistant.message") => {
                let text = d["content"].as_str().unwrap_or("");
                if text.trim().is_empty() {
                    return vec![];
                }
                self.final_text = Some(text.to_string());
                let id = d["messageId"].as_str().unwrap_or("m").to_string();
                vec![self.put(Step { id, kind: "text".into(), detail: text.into(), status: "done".into(), ..Default::default() })]
            }
            Some("tool.execution_start") => {
                let id = d["toolCallId"].as_str().unwrap_or("tool").to_string();
                let name = d["toolName"].as_str().unwrap_or("tool");
                vec![self.tool_start(&id, name, &d["arguments"], d["toolTitle"].as_str())]
            }
            Some("tool.execution_complete") => {
                let id = d["toolCallId"].as_str().unwrap_or("tool").to_string();
                let exit = d["shellExecution"]["exitCode"].as_i64().map(|c| c as i32);
                let ok = d["success"].as_bool().unwrap_or(true) && exit.is_none_or(|c| c == 0);
                let mut output = content_text(&d["result"]["content"]);
                if let Some(i) = output.find("\n<shellId:") {
                    output.truncate(i);
                }
                if output.is_empty() {
                    output = d["error"]["message"].as_str().unwrap_or("").to_string();
                }
                vec![self.tool_end(&id, output, ok, exit)]
            }
            Some("session.error") => {
                let msg = d["message"].as_str().unwrap_or("Error").to_string();
                vec![self.put(Step { id: format!("error-{}", self.steps.len()), kind: "error".into(), detail: msg, status: "error".into(), ..Default::default() })]
            }
            Some("result") => {
                // Peticiones premium y tokens del resultado; Copilot los informa en sitios distintos.
                let requests = v["premiumRequests"].as_f64()
                    .or_else(|| d["premiumRequests"].as_f64())
                    .or_else(|| v["usage"]["premiumRequests"].as_f64())
                    .or_else(|| d["usage"]["premiumRequests"].as_f64());
                let input = v["inputTokens"].as_u64()
                    .or_else(|| d["inputTokens"].as_u64())
                    .or_else(|| v["usage"]["inputTokens"].as_u64())
                    .or_else(|| d["usage"]["inputTokens"].as_u64());
                let output = v["outputTokens"].as_u64()
                    .or_else(|| d["outputTokens"].as_u64())
                    .or_else(|| v["usage"]["outputTokens"].as_u64())
                    .or_else(|| d["usage"]["outputTokens"].as_u64());
                self.usage.add(&Usage { input_tokens: input, output_tokens: output, cost_usd: None, requests });
                vec![]
            }
            _ => vec![],
        }
    }

    /// Id de la sesión de la CLI, si lo ha dicho (docs/plan-reanudar-sesiones.md §2).
    pub fn session_id(&self) -> Option<String> {
        self.session_id.clone()
    }

    /// Uso acumulado (tokens, coste, peticiones) de la CLI.
    pub fn usage(&self) -> Usage {
        self.usage.clone()
    }

    /// Respuesta final del agente: la que da la CLI o, si no, el último texto.
    pub fn final_text(&self) -> String {
        if let Some(t) = &self.final_text {
            return t.trim().to_string();
        }
        self.steps.iter().rev().find(|s| s.kind == "text").map(|s| s.detail.trim().to_string()).unwrap_or_default()
    }

    /// Pasos al terminar: los que quedaron «en curso» se dan por terminados.
    pub fn finish(mut self) -> Vec<Step> {
        for s in &mut self.steps {
            if s.status == "running" {
                s.status = "done".into();
            }
        }
        self.steps
    }
}

/// Id de sesión que trae un evento, según la CLI. Vale el último que llegue: agy, si no encuentra
/// la conversación pedida, abre otra y lo dice con su id.
fn session_of(format: Format, v: &Value) -> Option<String> {
    let s = |x: &Value| x.as_str().filter(|t| !t.is_empty()).map(str::to_string);
    match format {
        // Todos los eventos del stream-json llevan `session_id`.
        Format::Claude => s(&v["session_id"]),
        // {"type":"thread.started","thread_id":"…"}
        Format::Codex => (v["type"] == "thread.started").then(|| s(&v["thread_id"])).flatten(),
        // Cada evento lleva `sessionID` («ses_…»).
        Format::OpenCode => s(&v["sessionID"]),
        // {"event":"init","conversation_id":…} y {"event":"result","result":{"conversation_id":…}}
        Format::Agy => s(&v["conversation_id"]).or_else(|| s(&v["result"]["conversation_id"])),
        Format::Copilot => s(&v["sessionId"]).or_else(|| s(&v["data"]["sessionId"])),
        Format::Plain => None,
    }
}

/// Resumen de lo que hizo un agente, para dárselo como contexto al siguiente.
pub fn summary(steps: &[Step]) -> Option<String> {
    let actions: Vec<String> = steps
        .iter()
        .filter_map(|s| match s.kind.as_str() {
            "command" => Some(format!("ejecutó `{}`{}", tail(&s.detail, 120), s.exit_code.filter(|c| *c != 0).map(|c| format!(" (código {c})")).unwrap_or_default())),
            "edit" => Some(format!("editó {}", s.detail)),
            _ => None,
        })
        .collect();
    if actions.is_empty() {
        return None;
    }
    let shown: Vec<String> = actions.iter().take(15).cloned().collect();
    let more = if actions.len() > 15 { format!(" y {} acciones más", actions.len() - 15) } else { String::new() };
    Some(format!("(Acciones: {}{more})", shown.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(format: Format, lines: &[&str]) -> Parser {
        let mut p = Parser::new(format, Path::new("/repo"));
        for l in lines {
            p.feed(l);
        }
        p
    }

    #[test]
    fn claude_comando_y_texto() {
        let p = run(Format::Claude, &[
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls","description":"Listar"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"a.txt","is_error":false}]}}"#,
            r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t2","name":"Edit","input":{"file_path":"/repo/src/a.rs"}}]}}"#,
            r#"{"type":"assistant","message":{"id":"m3","content":[{"type":"text","text":"Hecho."}]}}"#,
            r#"{"type":"result","result":"Hecho.","is_error":false}"#,
        ]);
        assert_eq!(p.final_text(), "Hecho.");
        let steps = p.finish();
        assert_eq!(steps[0].kind, "command");
        assert_eq!(steps[0].detail, "ls");
        assert_eq!(steps[0].output.as_deref(), Some("a.txt"));
        assert_eq!(steps[1].kind, "edit");
        assert_eq!(steps[1].detail, "src/a.rs");
    }

    #[test]
    fn codex_comando_y_cambio() {
        let p = run(Format::Codex, &[
            r#"{"type":"item.started","item":{"id":"i1","type":"command_execution","command":"/bin/zsh -lc ls","aggregated_output":"","exit_code":null,"status":"in_progress"}}"#,
            r#"{"type":"item.completed","item":{"id":"i1","type":"command_execution","command":"/bin/zsh -lc ls","aggregated_output":"a.txt\n","exit_code":0,"status":"completed"}}"#,
            r#"{"type":"item.completed","item":{"id":"i2","type":"file_change","changes":[{"path":"/private/repo/a.txt","kind":"update"}],"status":"completed"}}"#,
            r#"{"type":"item.completed","item":{"id":"i3","type":"agent_message","text":"Listo."}}"#,
        ]);
        assert_eq!(p.final_text(), "Listo.");
        let steps = p.finish();
        assert_eq!((steps[0].detail.as_str(), steps[0].exit_code, steps[0].status.as_str()), ("ls", Some(0), "done"));
        assert_eq!(steps[1].detail, "a.txt");
    }

    #[test]
    fn agy_texto_por_partes() {
        let p = run(Format::Agy, &[
            r#"{"event":"step_update","step_update":{"step_index":2,"state":"DONE","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","parameters":{"CommandLine":"ls"},"output":"Command exited with code 0.\n\nCommand output:\na.txt\r\n"}}}"#,
            r#"{"event":"step_update","step_update":{"step_index":5,"state":"ACTIVE","step_type":"agent_response","text_delta":"Ho"}}"#,
            r#"{"event":"step_update","step_update":{"step_index":5,"state":"DONE","step_type":"agent_response","text_delta":"la"}}"#,
        ]);
        let steps = p.finish();
        assert_eq!((steps[0].kind.as_str(), steps[0].exit_code, steps[0].output.as_deref()), ("command", Some(0), Some("a.txt\n")));
        assert_eq!(steps[1].detail, "Hola");
    }

    #[test]
    fn copilot_parche() {
        let p = run(Format::Copilot, &[
            r#"{"type":"tool.execution_start","data":{"toolCallId":"c1","toolName":"apply_patch","arguments":"*** Begin Patch\n*** Update File: /repo/a.txt\n@@\n+adios\n*** End Patch\n","toolTitle":"Applying patch"}}"#,
            r#"{"type":"tool.execution_complete","data":{"toolCallId":"c1","success":true,"result":{"content":"ok"}}}"#,
            r#"{"type":"assistant.message","data":{"messageId":"m","content":"Añadido."}}"#,
        ]);
        assert_eq!(p.final_text(), "Añadido.");
        let steps = p.finish();
        assert_eq!((steps[0].kind.as_str(), steps[0].detail.as_str()), ("edit", "a.txt"));
    }

    #[test]
    fn claude_uso_y_coste() {
        let p = run(Format::Claude, &[
            r#"{"type":"result","total_cost_usd":0.0123,"usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":2000,"output_tokens":50}}"#,
        ]);
        let u = p.usage();
        assert_eq!(u.input_tokens, Some(2110));
        assert_eq!(u.output_tokens, Some(50));
        assert!((u.cost_usd.unwrap() - 0.0123).abs() < 1e-9);
    }

    #[test]
    fn codex_uso_acumulado() {
        let p = run(Format::Codex, &[
            r#"{"type":"turn.completed","usage":{"input_tokens":1200,"cached_input_tokens":800,"output_tokens":300}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":20}}"#,
        ]);
        let u = p.usage();
        assert_eq!(u.input_tokens, Some(1300));
        assert_eq!(u.output_tokens, Some(320));
        assert_eq!(u.cost_usd, None);
    }

    #[test]
    fn opencode_uso_del_paso() {
        let p = run(Format::OpenCode, &[
            r#"{"type":"step_finish","part":{"tokens":{"input":500,"output":40,"reasoning":10,"cache":{"read":100,"write":0}},"cost":0.002}}"#,
        ]);
        let u = p.usage();
        assert_eq!(u.input_tokens, Some(600));
        assert_eq!(u.output_tokens, Some(50));
        assert!((u.cost_usd.unwrap() - 0.002).abs() < 1e-9);
    }

    #[test]
    fn agy_uso_del_resultado() {
        let p = run(Format::Agy, &[
            r#"{"event":"result","result":{"status":"SUCCESS","response":"ok","usage":{"inputTokens":7,"outputTokens":3}}}"#,
        ]);
        let u = p.usage();
        assert_eq!(u.input_tokens, Some(7));
        assert_eq!(u.output_tokens, Some(3));
    }

    #[test]
    fn copilot_peticiones_premium() {
        let p = run(Format::Copilot, &[r#"{"type":"result","data":{"premiumRequests":1}}"#]);
        let u = p.usage();
        assert!((u.requests.unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(u.input_tokens, None);
    }

    #[test]
    fn uso_suma() {
        let mut u = Usage::default();
        u.add(&Usage::default());
        assert_eq!(u, Usage::default());
        u.add(&Usage { input_tokens: Some(2), ..Default::default() });
        assert_eq!(u.input_tokens, Some(2));
    }

    #[test]
    fn claude_sin_uso_deja_default() {
        let p = run(Format::Claude, &[r#"{"type":"result","result":"Hecho.","is_error":false}"#]);
        assert_eq!(p.usage(), Usage::default());
    }

    #[test]
    fn resumen_de_acciones() {
        let steps = vec![
            Step { id: "1".into(), kind: "command".into(), detail: "npm test".into(), exit_code: Some(1), status: "error".into(), ..Default::default() },
            Step { id: "2".into(), kind: "edit".into(), detail: "src/a.rs".into(), status: "done".into(), ..Default::default() },
        ];
        assert_eq!(summary(&steps).unwrap(), "(Acciones: ejecutó `npm test` (código 1); editó src/a.rs)");
    }

    #[test]
    fn id_de_sesion() {
        // Salidas reales del 04/11/2026 (recortadas).
        let p = run(Format::Claude, &[r#"{"type":"system","subtype":"init","session_id":"24d84a06-eb17-4bd5-8a13-cc72ffdf7558"}"#]);
        assert_eq!(p.session_id().as_deref(), Some("24d84a06-eb17-4bd5-8a13-cc72ffdf7558"));
        let p = run(Format::Codex, &[r#"{"type":"thread.started","thread_id":"01a1a59b-610e-7163-84d1-bdb9cf72e47b"}"#, r#"{"type":"turn.started"}"#]);
        assert_eq!(p.session_id().as_deref(), Some("01a1a59b-610e-7163-84d1-bdb9cf72e47b"));
        let p = run(Format::OpenCode, &[r#"{"type":"step_start","sessionID":"ses_e5a645116ffeqKJVIvnr9bfshc","part":{}}"#]);
        assert_eq!(p.session_id().as_deref(), Some("ses_e5a645116ffeqKJVIvnr9bfshc"));
        let p = run(Format::Agy, &[
            r#"{"event":"init","conversation_id":"735399fc-9c28-4db3-a3f1-aaf6f99e8b5b","init":{}}"#,
            r#"{"event":"result","result":{"conversation_id":"52b4d054-9472-4090-b391-0864f044366f","status":"SUCCESS","response":"Hola"}}"#,
        ]);
        assert_eq!(p.session_id().as_deref(), Some("52b4d054-9472-4090-b391-0864f044366f"));
        assert_eq!(run(Format::Codex, &[r#"{"type":"turn.started"}"#]).session_id(), None);
    }
}

/// Comprueba los traductores con salidas reales guardadas en una carpeta:
/// `AGENTES_FMT_DIR=… cargo test formatos_reales -- --ignored --nocapture`
#[cfg(test)]
#[test]
#[ignore]
fn formatos_reales() {
    let dir = std::env::var("AGENTES_FMT_DIR").expect("define AGENTES_FMT_DIR");
    let cwd = std::env::var("AGENTES_FMT_CWD").unwrap_or_default();
    for (name, format) in [("claude", Format::Claude), ("codex", Format::Codex), ("opencode", Format::OpenCode), ("agy", Format::Agy), ("copilot", Format::Copilot)] {
        let Ok(text) = std::fs::read_to_string(format!("{dir}/{name}.jsonl")) else { continue };
        let mut p = Parser::new(format, Path::new(&cwd));
        for l in text.lines() {
            p.feed(l);
        }
        println!("== {name} → «{}»", p.final_text());
        for s in p.finish() {
            println!("   [{}|{}] {} {}", s.kind, s.status, s.detail.replace('\n', " ⏎ ").chars().take(80).collect::<String>(), s.exit_code.map(|c| format!("(exit {c})")).unwrap_or_default());
        }
    }

}
