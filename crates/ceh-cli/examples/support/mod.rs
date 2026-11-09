use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct Server {
    pub port: u16,
    pub posts: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Server {
    pub fn new() -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let posts = Arc::new(AtomicUsize::new(0));
        let counter = posts.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                let mut data = vec![0; 65536];
                let Ok(length) = stream.read(&mut data) else {
                    continue;
                };
                let request = String::from_utf8_lossy(&data[..length]);
                let first = request.lines().next().unwrap_or("");
                if first.starts_with("POST ") {
                    counter.fetch_add(1, Ordering::SeqCst);
                    if first.contains("/lost ") {
                        continue;
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                let (body, mime) = if first.contains("/probe.js ") {
                    ("window.scriptLoaded=true;", "text/javascript")
                } else if first.contains("/applicant ") {
                    (
                        "<body><iframe src='/frame'></iframe><p>No hay citas disponibles</p><input type='text' id='txtIdCitado'><input type='text' name='fecha' style='display:none'><button id='btnEnviar'>Aceptar</button></body>",
                        "text/html",
                    )
                } else if first.contains("/expired ") {
                    ("<body>La sesión ha caducado</body>", "text/html")
                } else if first.contains("/frame ") {
                    ("<p id='frame'>frame</p>", "text/html")
                } else if first.starts_with("POST ") {
                    ("<h1 id='done'>done</h1>", "text/html")
                } else {
                    (
                        "<title>Local fixture</title><script src='/probe.js'></script><iframe src='/frame'></iframe><form method='post' action='/submit'><input name='fixture' value='synthetic'><button id='submit'>Submit</button></form>",
                        "text/html",
                    )
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {mime}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Ok(Self {
            port,
            posts,
            stop,
            thread: Some(thread),
        })
    }
}
