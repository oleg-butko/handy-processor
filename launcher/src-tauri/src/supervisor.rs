use std::{
    io::Write,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};

use crate::{
    health::{probe, ProbeResult},
    runtime::RuntimePaths,
};

const HEALTH_TIMEOUT: Duration = Duration::from_millis(500);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(12);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(4);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug)]
enum ServerMode {
    External,
    Managed(Child),
    Stopped,
}

#[derive(Debug)]
struct State {
    mode: ServerMode,
    exiting: bool,
}

#[derive(Clone, Debug)]
pub struct Supervisor {
    paths: RuntimePaths,
    state: Arc<Mutex<State>>,
}

impl Supervisor {
    pub fn initialize(paths: RuntimePaths) -> Result<Self> {
        let mode = match probe(paths.port, HEALTH_TIMEOUT) {
            ProbeResult::Compatible => ServerMode::External,
            ProbeResult::Unavailable => start_managed(&paths)?,
            ProbeResult::Incompatible(reason) => {
                bail!(
                    "port {} is occupied by an incompatible service: {reason}",
                    paths.port
                )
            }
        };

        let supervisor = Self {
            paths,
            state: Arc::new(Mutex::new(State {
                mode,
                exiting: false,
            })),
        };
        supervisor.start_monitor();
        Ok(supervisor)
    }

    pub fn restart(&self) -> Result<()> {
        let mut state = self.state.lock().expect("supervisor mutex poisoned");
        if state.exiting {
            bail!("launcher is shutting down");
        }

        let previous = std::mem::replace(&mut state.mode, ServerMode::Stopped);
        match previous {
            ServerMode::External => {
                state.mode = ServerMode::External;
                bail!("the server was started externally and is not owned by this launcher");
            }
            ServerMode::Managed(mut child) => stop_child(&mut child),
            ServerMode::Stopped => {}
        }

        state.mode = start_managed(&self.paths)?;
        Ok(())
    }

    pub fn shutdown(&self) {
        let mut state = self.state.lock().expect("supervisor mutex poisoned");
        if state.exiting {
            return;
        }
        state.exiting = true;

        if let ServerMode::Managed(mut child) =
            std::mem::replace(&mut state.mode, ServerMode::Stopped)
        {
            stop_child(&mut child);
        }
    }

    fn start_monitor(&self) {
        let state = Arc::downgrade(&self.state);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(1));
            let Some(state) = state.upgrade() else {
                break;
            };
            let mut state = state.lock().expect("supervisor mutex poisoned");
            if state.exiting {
                break;
            }

            let exited = match &mut state.mode {
                ServerMode::Managed(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        eprintln!("Managed server exited unexpectedly with {status}");
                        true
                    }
                    Ok(None) => false,
                    Err(error) => {
                        eprintln!("Cannot query managed server status: {error}");
                        true
                    }
                },
                _ => false,
            };
            if exited {
                state.mode = ServerMode::Stopped;
            }
        });
    }
}

fn start_managed(paths: &RuntimePaths) -> Result<ServerMode> {
    if !paths.sidecar.is_file() {
        bail!("server executable not found: {}", paths.sidecar.display());
    }

    let mut command = Command::new(&paths.sidecar);
    command
        .arg("--managed")
        .current_dir(&paths.directory)
        .env("HANDY_PROCESSOR_RUNTIME_DIR", &paths.directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("cannot start {}", paths.sidecar.display()))?;
    let deadline = Instant::now() + STARTUP_TIMEOUT;

    loop {
        match probe(paths.port, HEALTH_TIMEOUT) {
            ProbeResult::Compatible => {
                if child.try_wait()?.is_some() {
                    return Ok(ServerMode::External);
                }
                return Ok(ServerMode::Managed(child));
            }
            ProbeResult::Incompatible(reason) => {
                stop_child(&mut child);
                bail!(
                    "port {} became occupied by an incompatible service: {reason}",
                    paths.port
                );
            }
            ProbeResult::Unavailable => {}
        }

        if let Some(status) = child.try_wait()? {
            bail!("managed server exited before becoming ready: {status}");
        }
        if Instant::now() >= deadline {
            stop_child(&mut child);
            bail!("managed server did not become ready within 12 seconds");
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn stop_child(child: &mut Child) {
    if let Some(mut input) = child.stdin.take() {
        let _ = input.write_all(b"shutdown\n");
        let _ = input.flush();
    }

    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                eprintln!("Cannot wait for managed server: {error}");
                break;
            }
        }
    }

    if let Err(error) = child.kill() {
        eprintln!("Cannot force-stop managed server: {error}");
    }
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_does_not_keep_the_supervisor_state_alive() {
        let supervisor = Supervisor {
            paths: RuntimePaths {
                directory: "data".into(),
                sidecar: "server.exe".into(),
                port: 11_434,
            },
            state: Arc::new(Mutex::new(State {
                mode: ServerMode::External,
                exiting: false,
            })),
        };
        let state = Arc::downgrade(&supervisor.state);

        supervisor.start_monitor();
        drop(supervisor);

        assert!(state.upgrade().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn graceful_stop_sends_the_managed_shutdown_command() {
        let mut child = Command::new("sh")
            .args(["-c", "read command; test \"$command\" = shutdown"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();

        stop_child(&mut child);

        assert!(child.try_wait().unwrap().unwrap().success());
    }
}
