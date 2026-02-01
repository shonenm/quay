use anyhow::Result;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Bind and spawn TCP listeners on the given ports, returning their `JoinHandles`.
/// Binding failures are warned and skipped; returns Err only if no port could be bound.
pub async fn spawn_listeners(ports: Vec<u16>, http: bool) -> Result<Vec<JoinHandle<()>>> {
    let mut tasks = Vec::new();

    for port in &ports {
        let port = *port;
        match TcpListener::bind(format!("127.0.0.1:{port}")).await {
            Ok(listener) => {
                println!("Listening on :{port}");
                let task = tokio::spawn(accept_loop(listener, port, http));
                tasks.push(task);
            }
            Err(e) => {
                eprintln!("Warning: failed to bind :{port} — {e}");
            }
        }
    }

    if tasks.is_empty() {
        anyhow::bail!("Failed to bind any ports");
    }

    Ok(tasks)
}

pub async fn run(ports: Vec<u16>, http: bool) -> Result<()> {
    if ports.is_empty() {
        anyhow::bail!("No ports specified. Usage: quay dev listen <port1> <port2> ...");
    }

    let tasks = spawn_listeners(ports, http).await?;

    println!("Press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;
    println!("\nShutting down...");

    for task in tasks {
        task.abort();
    }

    Ok(())
}

async fn accept_loop(listener: TcpListener, port: u16, http: bool) {
    loop {
        match listener.accept().await {
            Ok((mut stream, addr)) => {
                if http {
                    let body = format!("quay dev listener on :{port}\n");
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.shutdown().await;
                }
                // Without --http, accept and drop (sufficient for probe detection)
                drop(stream);
                let _ = addr; // suppress unused warning in non-http mode
            }
            Err(e) => {
                eprintln!("Accept error on :{port}: {e}");
            }
        }
    }
}
