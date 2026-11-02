//! Terminal dentro de la app (pseudo-terminal real) para instalar agentes e iniciar sesión. La salida se envía a la ventana con el evento "term-output".

use crate::env::child_env;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

struct Session {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
pub struct Terminals {
    next: AtomicU32,
    sessions: Arc<Mutex<HashMap<u32, Session>>>,
}

pub struct Job {
    pub command: String,
    pub file: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

#[derive(Serialize, Clone)]
struct Output {
    id: u32,
    data: String,
}

#[derive(Serialize, Clone)]
struct Exit {
    id: u32,
    code: i32,
}

impl Terminals {
    pub fn start(&self, app: AppHandle, job: Job, cols: u16, rows: u16) -> Result<u32, String> {
        let pty = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
        let mut cmd = CommandBuilder::new(&job.file);
        cmd.args(&job.args);
        cmd.cwd(&job.cwd);
        cmd.env_clear();
        for (k, v) in child_env() {
            cmd.env(k, v);
        }
        cmd.env("TERM", "xterm-256color");
        let mut child = pty.slave.spawn_command(cmd).map_err(|e| format!("No se pudo abrir el terminal: {e}"))?;
        drop(pty.slave);

        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let mut reader = pty.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pty.master.take_writer().map_err(|e| e.to_string())?;
        let killer = child.clone_killer();
        self.sessions.lock().unwrap().insert(id, Session { writer, master: pty.master, killer });

        let _ = app.emit("term-output", Output { id, data: format!("\x1b[90m$ {}\x1b[0m\r\n", job.command) });

        // Lee la salida. Guarda los bytes de un carácter UTF-8 partido entre dos lecturas.
        let app_out = app.clone();
        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut pending: Vec<u8> = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.extend_from_slice(&buf[..n]);
                        let valid = match std::str::from_utf8(&pending) {
                            Ok(_) => pending.len(),
                            Err(e) if e.error_len().is_none() => e.valid_up_to(),
                            Err(_) => pending.len(),
                        };
                        let data = String::from_utf8_lossy(&pending[..valid]).to_string();
                        pending.drain(..valid);
                        let _ = app_out.emit("term-output", Output { id, data });
                    }
                }
            }
        });

        let sessions = self.sessions.clone();
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
            let _ = reader_thread.join();
            sessions.lock().unwrap().remove(&id);
            let _ = app.emit("term-exit", Exit { id, code });
        });
        Ok(id)
    }

    pub fn write(&self, id: u32, data: &str) {
        if let Some(s) = self.sessions.lock().unwrap().get_mut(&id) {
            let _ = s.writer.write_all(data.as_bytes());
            let _ = s.writer.flush();
        }
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) {
        if let Some(s) = self.sessions.lock().unwrap().get(&id) {
            let _ = s.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        }
    }

    pub fn kill(&self, id: u32) {
        if let Some(mut s) = self.sessions.lock().unwrap().remove(&id) {
            let _ = s.killer.kill();
        }
    }
}
