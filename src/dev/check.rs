use anyhow::Result;
use std::time::Duration;
use tokio::net::TcpStream;

pub async fn run(ports: Vec<u16>) -> Result<()> {
    if ports.is_empty() {
        anyhow::bail!("No ports specified. Usage: quay dev check <port1> <port2> ...");
    }

    let mut handles = Vec::new();
    for port in &ports {
        let port = *port;
        handles.push(tokio::spawn(async move {
            let addr = format!("127.0.0.1:{port}");
            let result =
                tokio::time::timeout(Duration::from_millis(200), TcpStream::connect(&addr)).await;
            let is_open = result.is_ok() && result.unwrap().is_ok();
            (port, is_open)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }
    results.sort_by_key(|(port, _)| *port);

    // Print table
    println!("{:<8} {:<6} STATUS", "PORT", "OPEN");
    println!("{}", "-".repeat(30));

    let mut open_count = 0;
    for (port, is_open) in &results {
        if *is_open {
            open_count += 1;
            println!(":{port:<7} \x1b[32m●\x1b[0m      open");
        } else {
            println!(":{port:<7} \x1b[90m○\x1b[0m      closed");
        }
    }

    println!();
    println!("{}/{} ports open", open_count, results.len());

    Ok(())
}
