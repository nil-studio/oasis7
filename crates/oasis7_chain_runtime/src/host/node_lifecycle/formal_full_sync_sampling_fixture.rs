//! Opaque, test-owned loopback forwarding: pause one real HTTP request after
//! the shipping worker has published begin_poll, without changing signed bytes.
use std::{
    io,
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

type PendingPause = (mpsc::Sender<()>, mpsc::Receiver<()>);

pub(super) struct RequestPause {
    entered: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
}
impl RequestPause {
    pub(super) fn wait_entered(&self) -> Result<(), String> {
        self.entered
            .recv_timeout(Duration::from_secs(30))
            .map_err(|error| {
                format!("next real full-sync HTTP request did not reach pause: {error}")
            })
    }
}
impl Drop for RequestPause {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

pub(super) struct SamplingProxy {
    pub(super) endpoint: String,
    pause: Arc<Mutex<Option<PendingPause>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), String>>>,
}
impl SamplingProxy {
    pub(super) fn start(upstream: &str) -> Result<Self, String> {
        let upstream = upstream
            .strip_prefix("http://")
            .ok_or("fixture requires loopback HTTP")?
            .to_owned();
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
        let endpoint = format!(
            "http://{}",
            listener.local_addr().map_err(|error| error.to_string())?
        );
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let pause = Arc::new(Mutex::new(None::<PendingPause>));
        let worker_pause = Arc::clone(&pause);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::SeqCst) {
                let client = match listener.accept() {
                    Ok((client, _)) => client,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => return Err(format!("sampling proxy accept: {error}")),
                };
                let pending = worker_pause
                    .lock()
                    .map_err(|error| error.to_string())?
                    .take();
                if let Some((entered, release)) = pending {
                    entered.send(()).map_err(|error| error.to_string())?;
                    release
                        .recv_timeout(Duration::from_secs(30))
                        .map_err(|error| {
                            format!("sampling pause exceeded its 30s budget: {error}")
                        })?;
                }
                forward(client, &upstream)?;
            }
            Ok(())
        });
        Ok(Self {
            endpoint,
            pause,
            stop,
            worker: Some(worker),
        })
    }
    pub(super) fn pause_next_request(&self) -> Result<RequestPause, String> {
        let (entered, wait_entered) = mpsc::channel();
        let (release, wait_release) = mpsc::channel();
        let mut pending = self.pause.lock().map_err(|error| error.to_string())?;
        if pending.is_some() {
            return Err("sampling proxy already has an armed pause".into());
        }
        *pending = Some((entered, wait_release));
        Ok(RequestPause {
            entered: wait_entered,
            release: Some(release),
        })
    }
    pub(super) fn stop(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| "sampling proxy worker panicked")??;
        }
        Ok(())
    }
}
impl Drop for SamplingProxy {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn forward(mut client: TcpStream, upstream: &str) -> Result<(), String> {
    client
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    client
        .set_read_timeout(Some(Duration::from_secs(60)))
        .map_err(|error| error.to_string())?;
    client
        .set_write_timeout(Some(Duration::from_secs(60)))
        .map_err(|error| error.to_string())?;
    let mut server = TcpStream::connect(upstream).map_err(|error| error.to_string())?;
    server
        .set_read_timeout(Some(Duration::from_secs(60)))
        .map_err(|error| error.to_string())?;
    server
        .set_write_timeout(Some(Duration::from_secs(60)))
        .map_err(|error| error.to_string())?;
    let mut request = client.try_clone().map_err(|error| error.to_string())?;
    let mut destination = server.try_clone().map_err(|error| error.to_string())?;
    let upload = thread::spawn(move || io::copy(&mut request, &mut destination));
    // The real formal HTTP service closes each response. Shutdown wakes the
    // upload clone even when reqwest keeps its client write half open.
    let response = io::copy(&mut server, &mut client);
    let _ = client.shutdown(Shutdown::Both);
    let _ = server.shutdown(Shutdown::Both);
    let uploaded = upload
        .join()
        .map_err(|_| "sampling proxy upload panicked")?;
    response.map_err(|error| format!("sampling proxy response: {error}"))?;
    uploaded.map_err(|error| format!("sampling proxy request: {error}"))?;
    Ok(())
}
