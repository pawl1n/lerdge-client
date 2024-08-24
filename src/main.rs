use std::{net::UdpSocket, thread::sleep, time::Duration};

const PORT: u16 = 34254;
const BROADCAST: &str = "255.255.255.255";
const TARGET_PORT: u16 = 8686;

fn main() -> std::io::Result<()> {
    let broadcast = format!("{}:{}", BROADCAST, TARGET_PORT);
    let local_ip = local_ip_address::local_ip().unwrap_or_else(|err| panic!("{}", err));
    let address = format!("{}:{}", local_ip, PORT);
    println!("Listening on: {}", address);
    let socket = UdpSocket::bind(address)?;

    let binding = local_ip.to_string();
    let ip_parts = binding.split('.').collect::<Vec<_>>();

    let data = format!(
        "<M888 A{} B{} C{} D{} P{}>\n",
        ip_parts[0], ip_parts[1], ip_parts[2], ip_parts[3], PORT
    );

    socket.set_broadcast(true)?;
    socket.send_to(data.as_bytes(), &broadcast)?;
    println!("Sent: {}", data);

    let mut buf = [0; 1024];

    loop {
        let (number_of_bytes, src_addr) = socket.recv_from(&mut buf)?;
        let received_data = String::from_utf8_lossy(&buf[..number_of_bytes]);

        let response = Response::from_data(&received_data);

        println!(
            "Received message from {}: {}\n Status: {:}",
            src_addr, response.body, response.status
        );

        if received_data.starts_with("echo: IP") {
            let message = "<M115>\n";
            socket.send_to(message.as_bytes(), src_addr)?;
            continue;
        }

        if received_data.trim().ends_with("ok") {
            let message = "M86\n";
            socket.send_to(message.as_bytes(), src_addr)?;
            sleep(Duration::from_millis(500));
        }
    }
}

#[derive(Debug)]
enum ResponseStatus {
    Ok,
    Error,
}

impl std::fmt::Display for ResponseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseStatus::Ok => write!(f, "Ok"),
            ResponseStatus::Error => write!(f, "Error"),
        }
    }
}

#[derive(Debug)]
struct Response {
    body: String,
    status: ResponseStatus,
}

impl Response {
    fn from_data(data: &str) -> Self {
        let trimmed = data.trim();
        let status = if trimmed.ends_with("ok") {
            ResponseStatus::Ok
        } else {
            ResponseStatus::Error
        };

        let body = trimmed[..trimmed.rfind('\n').unwrap_or(trimmed.len())].to_string();

        Self { body, status }
    }
}
