use crate::sanitize::sanitize_output;
use std::collections::HashSet;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

/// Пул активных процессов. По одному на entry_idx.
pub struct ProcessPool {
    running: Vec<RunningProcess>,
}

struct RunningProcess {
    entry_idx: usize,
    /// Для `live: true` — индекс live-записи в `App::runs`.
    run_id: Option<usize>,
    child: Child,
    started_at: SystemTime,
    started_instant: Instant,
    rx: Receiver<StreamChunk>,
    stdout_raw: Vec<u8>,
    stderr_raw: Vec<u8>,
}

enum StreamChunk {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Eof,
}

/// Обновление буферов ещё идущего live-процесса.
pub struct LiveOutput {
    pub run_id: usize,
    pub stdout: String,
    pub stderr: String,
}

/// Временная структура: завершившийся процесс, готовый стать / обновить RunRecord.
pub struct FinishedRun {
    pub entry_idx: usize,
    /// Если процесс был live — обновить эту запись, иначе создать новую.
    pub run_id: Option<usize>,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
    pub duration: Duration,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl ProcessPool {
    pub fn new() -> Self {
        Self { running: Vec::new() }
    }

    pub fn is_running(&self, entry_idx: usize) -> bool {
        self.running.iter().any(|rp| rp.entry_idx == entry_idx)
    }

    /// Переносит `entry_idx` активных процессов по карте `старый → новый`.
    /// Процесс, чей пункт исчез из меню, получает `usize::MAX` и больше
    /// не привязан ни к одному элементу.
    pub fn remap_entries(&mut self, old_to_new: &[Option<usize>]) {
        for rp in &mut self.running {
            rp.entry_idx = old_to_new
                .get(rp.entry_idx)
                .copied()
                .flatten()
                .unwrap_or(usize::MAX);
        }
    }

    /// Запускает `sh -c <cmd>` и подписывается на stdout/stderr.
    /// `run_id` — для live-режима (запись уже создана).
    pub fn spawn(
        &mut self,
        entry_idx: usize,
        cmd: &str,
        run_id: Option<usize>,
    ) -> std::io::Result<()> {
        let mut cmd_builder = Command::new("sh");
        cmd_builder
            .arg("-c")
            .arg(cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd_builder.process_group(0);
        }

        let mut child = cmd_builder.spawn()?;

        let (tx, rx) = mpsc::channel::<StreamChunk>();

        if let Some(mut out) = child.stdout.take() {
            let tx = tx.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match out.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if tx.send(StreamChunk::Stdout(buf[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let _ = tx.send(StreamChunk::Eof);
            });
        } else {
            let _ = tx.send(StreamChunk::Eof);
        }
        if let Some(mut err) = child.stderr.take() {
            let tx = tx.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match err.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if tx.send(StreamChunk::Stderr(buf[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let _ = tx.send(StreamChunk::Eof);
            });
        } else {
            let _ = tx.send(StreamChunk::Eof);
        }
        drop(tx);

        let started_at = SystemTime::now();
        let started_instant = Instant::now();
        self.running.push(RunningProcess {
            entry_idx,
            run_id,
            child,
            started_at,
            started_instant,
            rx,
            stdout_raw: Vec::new(),
            stderr_raw: Vec::new(),
        });
        Ok(())
    }

    /// Дренирует каналы: отдаёт снимки live-процессов и завершившиеся.
    pub fn poll(&mut self) -> (Vec<LiveOutput>, Vec<FinishedRun>) {
        let mut live: Vec<LiveOutput> = Vec::new();
        let mut finished: Vec<FinishedRun> = Vec::new();

        for rp in self.running.iter_mut() {
            let mut changed = false;
            loop {
                match rp.rx.try_recv() {
                    Ok(StreamChunk::Stdout(chunk)) => {
                        rp.stdout_raw.extend_from_slice(&chunk);
                        changed = true;
                    }
                    Ok(StreamChunk::Stderr(chunk)) => {
                        rp.stderr_raw.extend_from_slice(&chunk);
                        changed = true;
                    }
                    Ok(StreamChunk::Eof) => {}
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            }

            match rp.child.try_wait() {
                Ok(Some(status)) => {
                    // Дочитываем остаток канала.
                    loop {
                        match rp.rx.try_recv() {
                            Ok(StreamChunk::Stdout(chunk)) => {
                                rp.stdout_raw.extend_from_slice(&chunk);
                            }
                            Ok(StreamChunk::Stderr(chunk)) => {
                                rp.stderr_raw.extend_from_slice(&chunk);
                            }
                            Ok(StreamChunk::Eof) => {}
                            Err(_) => break,
                        }
                    }

                    let exit_code = status.code();
                    #[cfg(unix)]
                    let signal = status.signal();
                    #[cfg(not(unix))]
                    let signal: Option<i32> = None;

                    finished.push(FinishedRun {
                        entry_idx: rp.entry_idx,
                        run_id: rp.run_id,
                        started_at: rp.started_at,
                        finished_at: SystemTime::now(),
                        duration: rp.started_instant.elapsed(),
                        exit_code,
                        signal,
                        ok: exit_code == Some(0),
                        stdout: sanitize_bytes(&rp.stdout_raw),
                        stderr: sanitize_bytes(&rp.stderr_raw),
                    });
                }
                Ok(None) => {
                    if changed {
                        if let Some(run_id) = rp.run_id {
                            live.push(LiveOutput {
                                run_id,
                                stdout: sanitize_bytes(&rp.stdout_raw),
                                stderr: sanitize_bytes(&rp.stderr_raw),
                            });
                        }
                    }
                }
                Err(e) => {
                    finished.push(FinishedRun {
                        entry_idx: rp.entry_idx,
                        run_id: rp.run_id,
                        started_at: rp.started_at,
                        finished_at: SystemTime::now(),
                        duration: rp.started_instant.elapsed(),
                        exit_code: None,
                        signal: None,
                        ok: false,
                        stdout: sanitize_bytes(&rp.stdout_raw),
                        stderr: format!("wait error: {e}"),
                    });
                }
            }
        }

        if !finished.is_empty() {
            let finished_idxs: HashSet<usize> =
                finished.iter().map(|f| f.entry_idx).collect();
            self.running
                .retain(|rp| !finished_idxs.contains(&rp.entry_idx));
        }

        (live, finished)
    }
}

fn sanitize_bytes(raw: &[u8]) -> String {
    sanitize_output(&String::from_utf8_lossy(raw))
}
