use std::{collections::HashMap, net::UdpSocket};

const PORT: u16 = 0; // Port to listen on
const BROADCAST: &str = "255.255.255.255"; // Broadcast address
const TARGET_PORT: u16 = 8686; // Target printer port

fn main() {
    let broadcast = format!("{}:{}", BROADCAST, TARGET_PORT);
    let local_ip = local_ip_address::local_ip().expect("Could not get local IP address");
    let server = Server::new(local_ip.to_string(), PORT, broadcast);
    let mut main_interface = MainInterface {
        server,
        ..Default::default()
    };

    main_interface.run();
}

#[derive(Debug)]
struct Server {
    local_ip: String,
    socket: UdpSocket,
    broadcast: String,
}

impl Default for Server {
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

impl Server {
    fn new(local_ip: String, port: u16, broadcast: String) -> Self {
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

    fn send_broadcast_message(&mut self, message: &str) -> std::io::Result<Response> {
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

    fn send_message(&self, message: &str, address: &str) -> std::io::Result<Response> {
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

    fn read_message(&self) -> std::io::Result<Response> {
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

#[derive(Debug, Clone)]
struct Printer {
    info: HashMap<String, String>,
    address: String,
}

#[derive(Debug)]
struct PrinterInterface<'a> {
    server: &'a mut Server,
    printer: Printer,
}

impl<'a> PrinterInterface<'a> {
    fn new(server: &'a mut Server, printer: Printer) -> Self {
        Self { server, printer }
    }

    fn run(&mut self) {
        loop {
            println!();
            println!("Printer interface ({})", self.printer.address);
            println!("1. Get stats");
            println!("2. Send command");
            println!("3. Send file");
            println!("0. Exit");

            let mut input = String::new();
            println!();
            println!("Select an option: ");
            std::io::stdin()
                .read_line(&mut input)
                .expect("Failed to read input");
            let index = input.trim().parse::<usize>();

            println!();

            match index {
                Ok(1) => self.show_stats(),
                Ok(2) => self.send_message(),
                Ok(3) => self.send_file(),
                Ok(0) => break,
                _ => println!("Invalid option"),
            }
        }
    }

    fn show_stats(&self) {
        let message = "M86\n";
        self.server
            .send_message(message, &self.printer.address)
            .map_or_else(
                |_| println!("Failed to get stats"),
                |response| println!("Stats: {}", response.body),
            );
    }

    fn send_message(&self) {
        let mut message = String::new();
        println!("Enter message to send: ");
        std::io::stdin()
            .read_line(&mut message)
            .expect("Failed to read input");
        self.server
            .send_message(&message, &self.printer.address)
            .map_or_else(
                |_| println!("Failed to send message"),
                |response| println!("Message sent: {}", response.body),
            );
    }

    fn send_file(&self) {
        println!("Enter file path: ");
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input");

        let file_path = input.trim();
        let filename = file_path.split('/').last().expect("Invalid file path");
        let size = std::fs::metadata(file_path).expect("File not found").len();

        let message = format!("M828 P{size} U:{filename}\n\n"); // Create file

        self.server
            .send_message(&message, &self.printer.address)
            .map_or_else(
                |_| println!("Failed to create file"),
                |response| println!("File created: {}", response.body),
            );

        let chunk_size = 1442; // Chunk size in bytes (1442 as in Lerdge official program)
        let mut start = 0;

        use std::io::{Read, Seek, SeekFrom};

        let mut f = std::fs::File::open(file_path).expect("Failed to open file");

        // Wait for ok message
        let response = self.server.read_message();
        if let Ok(response) = response {
            println!("{}", response.body);
        }

        let mut repeat = true;

        while start < size {
            let count = std::cmp::min(chunk_size, size - start) as usize;
            println!("Sending {}-{} of {}", start, start + count as u64, size);

            start = f.seek(SeekFrom::Start(start)).expect("Failed to seek") + count as u64;
            let mut buf = vec![0; count];
            f.read_exact(&mut buf).expect("Failed to read file");

            let chunk = format!("\n\n\n\n\n\n\nU{}", String::from_utf8_lossy(&buf[..count]));

            let response = self.server.send_message(&chunk, &self.printer.address);

            match response {
                Ok(response) => match response.status {
                    ResponseStatus::Ok => println!("Chunk sent"),
                    _ => {
                        println!("Error sending chunk: {}", response.body);
                        break;
                    }
                },
                Err(e) => {
                    eprintln!("Error sending chunk: {}", e);
                    repeat = true;
                }
            }

            if repeat {
                start = 0;
                repeat = false;
            }
        }
    }
}

#[derive(Debug, Default)]
struct MainInterface {
    server: Server,
    selected_printer: Option<Printer>,
    available_printers: Vec<Printer>,
}

impl MainInterface {
    fn run(&mut self) {
        loop {
            println!();
            println!("Main interface");
            println!("1. Search for printers");
            println!("2. Select a printer");
            println!("0. Exit");

            let mut input = String::new();
            println!();
            println!("Select an option: ");
            std::io::stdin()
                .read_line(&mut input)
                .expect("Failed to read input");
            let index = input.trim().parse::<usize>();

            println!();

            match index {
                Ok(1) => self.search(),
                Ok(2) => self.select_printer(),
                Ok(0) => break,
                _ => println!("Invalid option"),
            }
        }
    }

    fn search(&mut self) {
        self.available_printers.clear();

        println!("Searching for printers...");
        let ip_parts = self.server.local_ip.split('.').collect::<Vec<_>>();

        let data = format!(
            "M888 A{} B{} C{} D{} P{}\n",
            ip_parts[0],
            ip_parts[1],
            ip_parts[2],
            ip_parts[3],
            self.server
                .socket
                .local_addr()
                .expect("Could not get socket address")
                .port()
        );
        let response = self
            .server
            .send_broadcast_message(&data)
            .expect("Error sending message");

        self.available_printers.push(Printer {
            address: response.address,
            info: Default::default(),
        });

        println!("Found {} printers", self.available_printers.len());
    }

    fn read_printer_number(&self) -> usize {
        let mut input = String::new();

        loop {
            std::io::stdin()
                .read_line(&mut input)
                .expect("Failed to read input");

            let index_result = input.trim().parse::<usize>();

            match index_result {
                Ok(i) => {
                    if i < self.available_printers.len() {
                        return i;
                    } else {
                        println!("Invalid option");
                    }
                }
                Err(_) => println!("Invalid number"),
            }

            input.clear();
        }
    }

    fn select_printer(&mut self) {
        if self.available_printers.is_empty() {
            println!("No printers found");
            return;
        }

        println!("Select a printer:");

        for (i, printer) in self.available_printers.iter().enumerate() {
            println!("{}: {}", i, printer.address);
        }

        let index = self.read_printer_number();

        let message = "M115\n";

        self.selected_printer = self
            .server
            .send_message(message, &self.available_printers[index].address)
            .map_or_else(
                |_| {
                    println!("Failed to get printer info");
                    None
                },
                |response| {
                    let mut printer = Printer {
                        info: HashMap::new(),
                        address: response.address.to_string(),
                    };
                    printer.info = response
                        .body
                        .split('\r')
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| {
                            let cleaned = s.replace('\n', "");
                            let parts = cleaned.trim().split(':').collect::<Vec<_>>();
                            if parts.len() != 2 {
                                println!("Invalid printer info: {}", cleaned);
                                return ("".to_string(), "".to_string());
                            }

                            let key = parts[0].trim().to_string();
                            let value = parts[1].trim().to_string();

                            (key, value)
                        })
                        .collect();
                    println!("Selected printer: {}", printer.address);
                    println!("Info:");
                    for (key, value) in printer.info.iter() {
                        println!("{}: {}", key, value);
                    }
                    Some(printer)
                },
            );

        if let Some(printer) = &self.selected_printer {
            let mut printer_interface = PrinterInterface::new(&mut self.server, printer.clone());
            printer_interface.run();
        }
    }
}

#[derive(Debug, PartialEq)]
enum ResponseStatus {
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
struct Response {
    address: String,
    body: String,
    status: ResponseStatus,
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
