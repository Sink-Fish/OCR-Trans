//! Run native model code in a reusable child. Only the supervisor can enforce a deadline
//! while a driver is inside a native call, or recover from a process-level fault.
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Mutex, TryLockError};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub model: PathBuf,
    pub cpu: bool,
    pub paragraphs: Vec<(String, String)>,
    pub glossary: Vec<(String, String)>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub translations: Vec<Option<String>>,
    pub device: Option<String>,
    pub gpu_failure: Option<String>,
}

#[derive(Serialize, Deserialize)]
enum Message {
    Progress(usize, usize),
    Finished(std::result::Result<Reply, String>),
}

fn send(message: &Message) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, message)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

pub fn run() -> Result<()> {
    // An orphan must also stop if its parent exits during generation or model loading.
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                return;
            }
        }
        ocrtrans_core::exit_now(0);
    });
    let mut model = None;
    let mut loaded = None;
    for line in rx {
        let result = (|| -> Result<Reply> {
            let request: Request = serde_json::from_str(&line)?;
            let key = (request.model, request.cpu);
            if loaded.as_ref() != Some(&key) {
                drop(model.take());
                loaded = None;
                ocrtrans_core::set_llm_device(key.1, None);
                model = Some(ocrtrans_core::llm::LlmTranslator::load(&key.0)?);
                loaded = Some(key);
            }
            let paragraphs = request
                .paragraphs
                .iter()
                .map(|(lang, text)| (lang.as_str(), text.as_str()))
                .collect::<Vec<_>>();
            let output = model
                .as_ref()
                .ok_or_else(|| anyhow!("模型未加载"))?
                .translate_reporting(&paragraphs, &request.glossary, |done, total| {
                    let _ = send(&Message::Progress(done, total));
                })?;
            let (device, gpu_failure) = ocrtrans_core::llm_device_state();
            Ok(Reply {
                translations: output.translations,
                device,
                gpu_failure,
            })
        })();
        send(&Message::Finished(result.map_err(|e| format!("{e:#}"))))?;
    }
    Ok(())
}

struct Worker {
    cpu: bool,
    child: Child,
    requests: mpsc::SyncSender<Vec<u8>>,
    messages: mpsc::Receiver<Message>,
    last_used: Instant,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Worker {
    fn start(cpu: bool) -> Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--quality-worker");
        if cpu { command.env("GGML_METAL_DEVICES", "0"); }
        Self::from_command(command, cpu)
    }

    fn from_command(mut command: Command, cpu: bool) -> Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn()?;
        let mut input = child.stdin.take().expect("piped stdin");
        let output = child.stdout.take().expect("piped stdout");
        let errors = child.stderr.take().expect("piped stderr");
        let (requests, rx) = mpsc::sync_channel::<Vec<u8>>(1);
        let (tx, messages) = mpsc::channel();
        std::thread::spawn(move || {
            for bytes in rx {
                if input
                    .write_all(&bytes)
                    .and_then(|_| input.write_all(b"\n"))
                    .and_then(|_| input.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            for line in BufReader::new(output)
                .lines()
                .map_while(std::result::Result::ok)
            {
                let Ok(message) = serde_json::from_str(&line) else {
                    break;
                };
                if tx.send(message).is_err() {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            for line in BufReader::new(errors)
                .lines()
                .map_while(std::result::Result::ok)
            {
                crate::log_info(&format!("quality worker: {line}"));
            }
        });
        Ok(Self {
            cpu,
            child,
            requests,
            messages,
            last_used: Instant::now(),
        })
    }
}

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);
static DEVICE: Mutex<Option<String>> = Mutex::new(None);

pub fn device() -> Option<String> {
    DEVICE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn release() {
    WORKER.lock().unwrap_or_else(|e| e.into_inner()).take();
    DEVICE.lock().unwrap_or_else(|e| e.into_inner()).take();
}

pub fn idle(minutes: u32) {
    if minutes > 0 {
        if let Ok(mut slot) = WORKER.try_lock() {
            if slot.as_ref().is_some_and(|w| {
                w.last_used.elapsed() >= Duration::from_secs(u64::from(minutes) * 60)
            }) {
                slot.take();
                DEVICE.lock().unwrap_or_else(|e| e.into_inner()).take();
            }
        }
    }
}

pub fn translate(
    request: &Request,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    progress: impl Fn(usize, usize),
) -> Result<Reply> {
    let check = || -> Result<()> {
        if cancelled() {
            bail!("翻译已取消");
        }
        if Instant::now() >= deadline {
            bail!("高质量翻译已超时");
        }
        Ok(())
    };
    let mut slot = loop {
        check()?;
        match WORKER.try_lock() {
            Ok(lock) => break lock,
            Err(TryLockError::Poisoned(e)) => break e.into_inner(),
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(25)),
        }
    };
    if slot.as_ref().is_some_and(|w| w.cpu != request.cpu) { slot.take(); }
    if slot.is_none() {
        *slot = Some(Worker::start(request.cpu)?);
    }
    let worker = slot.as_mut().expect("started worker");
    let result = (|| -> Result<Reply> {
        worker.requests.try_send(serde_json::to_vec(request)?)?;
        loop {
            check()?;
            match worker.messages.recv_timeout(Duration::from_millis(25)) {
                Ok(Message::Progress(done, total)) => progress(done, total),
                Ok(Message::Finished(reply)) => return reply.map_err(|e| anyhow!(e)),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("翻译进程意外退出 ({:?})", worker.child.try_wait()?)
                }
            }
        }
    })();
    if let Ok(reply) = &result {
        worker.last_used = Instant::now();
        *DEVICE.lock().unwrap_or_else(|e| e.into_inner()) = reply.device.clone();
    } else {
        slot.take();
        DEVICE.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn simulated(script: &str) {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", script]);
        *WORKER.lock().unwrap() = Some(Worker::from_command(cmd, true).unwrap());
    }

    #[test]
    fn native_crash_timeout_cancel_and_next_request() {
        let request = Request {
            model: PathBuf::new(),
            cpu: true,
            paragraphs: vec![],
            glossary: vec![],
        };
        let deadline = || Instant::now() + Duration::from_secs(2);
        simulated("read -r request; exit 9");
        assert!(translate(&request, deadline(), || false, |_, _| {})
            .unwrap_err()
            .to_string()
            .contains("意外退出"));
        simulated("read -r request; exec sleep 60");
        assert!(translate(
            &request,
            Instant::now() + Duration::from_millis(80),
            || false,
            |_, _| {}
        )
        .unwrap_err()
        .to_string()
        .contains("超时"));
        simulated("read -r request; exec sleep 60");
        let started = Instant::now();
        assert!(translate(
            &request,
            deadline(),
            || started.elapsed() > Duration::from_millis(80),
            |_, _| {}
        )
        .unwrap_err()
        .to_string()
        .contains("取消"));
        assert!(WORKER.lock().unwrap().is_none());
        simulated(
            r#"read -r request; echo '{"Finished":{"Ok":{"translations":["保存"],"device":"CPU","gpu_failure":null}}}'"#,
        );
        assert_eq!(
            translate(&request, deadline(), || false, |_, _| {})
                .unwrap()
                .translations,
            [Some("保存".into())]
        );
        release();
    }
}
