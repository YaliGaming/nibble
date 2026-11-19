use nibble_core::ipc::pipe_path;
use serde_json::{json, Value};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::watch;

pub type Closed = watch::Receiver<bool>;
pub type Reply = Pin<Box<dyn Future<Output = Value> + Send>>;
pub type Handler = Arc<dyn Fn(String, Option<Value>, Closed) -> Reply + Send + Sync>;

#[cfg(windows)]
pub fn listen(home: &Path, handler: Handler) -> std::io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let name = pipe_path(home);
    tauri::async_runtime::spawn(async move {
        let Ok(mut server) = ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(&name) else {
            return;
        };
        loop {
            if server.connect().await.is_err() {
                break;
            }
            let connected = server;
            server = match ServerOptions::new().reject_remote_clients(true).create(&name) {
                Ok(next) => next,
                Err(_) => break,
            };
            let handler = handler.clone();
            tokio::spawn(serve(connected, handler));
        }
    });
    Ok(())
}

#[cfg(not(windows))]
pub fn listen(_home: &Path, _handler: Handler) -> std::io::Result<()> {
    Ok(())
}

async fn serve<S>(stream: S, handler: Handler)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    let (read, mut write) = tokio::io::split(stream);
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    match reader.read_line(&mut line).await {
        Ok(0) | Err(_) => return,
        Ok(_) => {}
    }
    if line.len() > 20000 {
        return;
    }
    let line = line.trim().to_string();
    let (word, payload) = match line.find(' ') {
        Some(space) if space > 0 => (line[..space].to_string(), serde_json::from_str::<Value>(&line[space + 1..]).ok()),
        _ => (line.clone(), None),
    };
    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        let mut sink = String::new();
        loop {
            sink.clear();
            match reader.read_line(&mut sink).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
        let _ = tx.send(true);
    });
    let reply = handler(word, payload, rx).await;
    let mut text = serde_json::to_string(&reply).unwrap_or_else(|_| json!({ "ok": false }).to_string());
    text.push('\n');
    let _ = write.write_all(text.as_bytes()).await;
    let _ = write.shutdown().await;
}
