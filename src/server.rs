use std::net::UdpSocket;

#[derive(Debug)]
pub struct UdpServer {
    pub local_ip: String,
    pub socket: UdpSocket,
    pub broadcast: String,
}

impl Default for UdpServer {
    fn default() -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").expect("Could not bind to address");
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("set_read_timeout call failed");

        Self {
            local_ip: "127.0.0.1".to_string(),
            socket,
            broadcast: "255.255.255.255:8686".to_string(),
        }
    }
}

impl UdpServer {
    pub fn new(local_ip: String, port: u16, broadcast: String) -> Self {
        let address = format!("{}:{}", local_ip, port);
        let socket = UdpSocket::bind(address).expect("Could not bind to address");
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .expect("set_read_timeout call failed");

        Self {
            local_ip,
            socket,
            broadcast,
        }
    }

    pub fn send_broadcast_message(&mut self, message: &str) -> std::io::Result<Response> {
        self.socket.set_broadcast(true)?;
        self.socket.send_to(message.as_bytes(), &self.broadcast)?;
        println!("Sent: {}", message);

        let mut buf = [0; 1024];

        let (number_of_bytes, src_addr) = self.socket.recv_from(&mut buf)?;
        let received_data = String::from_utf8_lossy(&buf[..number_of_bytes]);

        let response = Response::from_data(src_addr.to_string(), &received_data);

        println!(
            "Received message from {}: {}\n Status: {:}",
            src_addr, response.body, response.status
        );

        Ok(response)
    }

    pub fn send_message(&self, message: &str, address: &str) -> std::io::Result<Response> {
        self.socket.send_to(message.as_bytes(), address)?;

        let mut buf = [0; 1024];

        let (number_of_bytes, src_addr) = self.socket.recv_from(&mut buf)?;
        let received_data = String::from_utf8_lossy(&buf[..number_of_bytes]);

        let response = Response::from_data(src_addr.to_string(), &received_data);

        println!(
            "Received message from {}: {}\n Status: {:}",
            src_addr, response.body, response.status
        );

        Ok(response)
    }

    pub fn read_message(&self) -> std::io::Result<Response> {
        let mut buf = [0; 1024];

        let (number_of_bytes, src_addr) = self.socket.recv_from(&mut buf)?;
        let received_data = String::from_utf8_lossy(&buf[..number_of_bytes]);

        let response = Response::from_data(src_addr.to_string(), &received_data);

        println!(
            "Received message from {}: {}\n Status: {:}",
            src_addr, response.body, response.status
        );

        Ok(response)
    }

    pub fn send_bytes(&self, bytes: &[u8], address: &str) -> std::io::Result<Response> {
        self.socket.send_to(bytes, address)?;

        let mut buf = [0; 1024];

        let (number_of_bytes, src_addr) = self.socket.recv_from(&mut buf)?;
        let received_data = String::from_utf8_lossy(&buf[..number_of_bytes]);

        let response = Response::from_data(src_addr.to_string(), &received_data);

        println!(
            "Received message from {}: {}\n Status: {:}",
            src_addr, response.body, response.status
        );

        Ok(response)
    }
}

#[derive(Debug, PartialEq)]
pub enum ResponseStatus {
    Ok,
    Error,
    Unknown,
}

impl std::fmt::Display for ResponseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseStatus::Ok => write!(f, "Ok"),
            ResponseStatus::Error => write!(f, "Error"),
            ResponseStatus::Unknown => write!(f, "Error"),
        }
    }
}

#[derive(Debug)]
pub struct Response {
    pub address: String,
    pub body: String,
    pub status: ResponseStatus,
}

impl Response {
    fn from_data(address: String, data: &str) -> Self {
        let trimmed = data.trim();
        let status = if trimmed.contains("error") {
            ResponseStatus::Error
        } else if trimmed.ends_with("ok") {
            ResponseStatus::Ok
        } else {
            ResponseStatus::Unknown
        };

        let body = trimmed[..trimmed.rfind('\n').unwrap_or(trimmed.len())].to_string();

        Self {
            address,
            body,
            status,
        }
    }
}
