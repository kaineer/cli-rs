use std::collections::HashSet;
use std::io::{BufRead, BufReader};
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
    child: Child,
    started_at: SystemTime,
    started_instant: Instant,
    rx: Receiver<StreamLine>,
    stdout_buf: String,
    stderr_buf: String,
}

enum StreamLine {
    Stdout(String),
    Stderr(String),
    Eof,
}

/// Временная структура: завершившийся процесс, готовый стать RunRecord.
pub struct FinishedRun {
    pub entry_idx: usize,
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

    /// Запускает `sh -c <cmd>` и подписывается на stdout/stderr.
    pub fn spawn(&mut self, entry_idx: usize, cmd: &str) -> std::io::Result<()> {
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

        let (tx, rx) = mpsc::channel::<StreamLine>();

        if let Some(out) = child.stdout.take() {
            let tx = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(out);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            let _ = tx.send(StreamLine::Stdout(l));
                        }
                        Err(_) => break,
                    }
                }
                let _ = tx.send(StreamLine::Eof);
            });
        }
        if let Some(err) = child.stderr.take() {
            let tx = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(err);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            let _ = tx.send(StreamLine::Stderr(l));
                        }
                        Err(_) => break,
                    }
                }
                let _ = tx.send(StreamLine::Eof);
            });
        }
        drop(tx);

        let started_at = SystemTime::now();
        let started_instant = Instant::now();
        self.running.push(RunningProcess {
            entry_idx,
            child,
            started_at,
            started_instant,
            rx,
            stdout_buf: String::new(),
            stderr_buf: String::new(),
        });
        Ok(())
    }

    /// Дренирует каналы всех процессов и собирает завершившиеся.
    pub fn poll(&mut self) -> Vec<FinishedRun> {
        let mut finished: Vec<FinishedRun> = Vec::new();

        for rp in self.running.iter_mut() {
            loop {
                match rp.rx.try_recv() {
                    Ok(StreamLine::Stdout(l)) => {
                        rp.stdout_buf.push_str(&l);
                        rp.stdout_buf.push('\n');
                    }
                    Ok(StreamLine::Stderr(l)) => {
                        rp.stderr_buf.push_str(&l);
                        rp.stderr_buf.push('\n');
                    }
                    Ok(StreamLine::Eof) => {}
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            }

            match rp.child.try_wait() {
                Ok(Some(status)) => {
                    for _ in 0..5 {
                        match rp.rx.try_recv() {
                            Ok(StreamLine::Stdout(l)) => {
                                rp.stdout_buf.push_str(&l);
                                rp.stdout_buf.push('\n');
                            }
                            Ok(StreamLine::Stderr(l)) => {
                                rp.stderr_buf.push_str(&l);
                                rp.stderr_buf.push('\n');
                            }
                            _ => break,
                        }
                    }

                    let exit_code = status.code();
                    #[cfg(unix)]
                    let signal = status.signal();
                    #[cfg(not(unix))]
                    let signal: Option<i32> = None;

                    finished.push(FinishedRun {
                        entry_idx: rp.entry_idx,
                        started_at: rp.started_at,
                        finished_at: SystemTime::now(),
                        duration: rp.started_instant.elapsed(),
                        exit_code,
                        signal,
                        ok: exit_code == Some(0),
                        stdout: std::mem::take(&mut rp.stdout_buf),
                        stderr: std::mem::take(&mut rp.stderr_buf),
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    finished.push(FinishedRun {
                        entry_idx: rp.entry_idx,
                        started_at: rp.started_at,
                        finished_at: SystemTime::now(),
                        duration: rp.started_instant.elapsed(),
                        exit_code: None,
                        signal: None,
                        ok: false,
                        stdout: std::mem::take(&mut rp.stdout_buf),
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

        finished
    }
}
