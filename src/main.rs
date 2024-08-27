use std::{collections::HashMap, net::UdpSocket};

const PORT: u16 = 34254; // Port to listen on
const BROADCAST: &str = "255.255.255.255"; // Broadcast address
const TARGET_PORT: u16 = 8686; // Target printer port

fn main() {
    let broadcast = format!("{}:{}", BROADCAST, TARGET_PORT);
    let local_ip = local_ip_address::local_ip().unwrap_or_else(|err| panic!("{}", err));
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
    port: u16,
    socket: UdpSocket,
    broadcast: String,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            local_ip: "127.0.0.1".to_string(),
            port: 0,
            socket: UdpSocket::bind("0.0.0.0:0").unwrap(),
            broadcast: "255.255.255.255:8686".to_string(),
        }
    }
}

impl Server {
    fn new(local_ip: String, port: u16, broadcast: String) -> Self {
        let address = format!("{}:{}", local_ip, port);
        let socket = UdpSocket::bind(address).expect("Could not bind to address");

        Self {
            local_ip,
            port,
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
            println!("0. Exit");

            let mut input = String::new();
            println!();
            println!("Select an option: ");
            std::io::stdin()
                .read_line(&mut input)
                .expect("Failed to read input");
            let index = input.trim().parse::<usize>().unwrap();

            println!();

            match index {
                1 => self.show_stats(),
                2 => self.send_message(),
                0 => break,
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
            // println!("3. Connect manually");
            println!("0. Exit");

            let mut input = String::new();
            println!();
            println!("Select an option: ");
            std::io::stdin()
                .read_line(&mut input)
                .expect("Failed to read input");
            let index = input.trim().parse::<usize>().unwrap();

            println!();

            match index {
                1 => self.search(),
                2 => self.select_printer(),
                // 3 => self.show_stats(),
                // 4 => self.connect_manually(),
                0 => break,
                _ => println!("Invalid option"),
            }
        }
    }

    fn search(&mut self) {
        println!("Searching for printers...");
        let ip_parts = self.server.local_ip.split('.').collect::<Vec<_>>();

        let data = format!(
            "<M888 A{} B{} C{} D{} P{}>\n",
            ip_parts[0], ip_parts[1], ip_parts[2], ip_parts[3], self.server.port
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

    fn select_printer(&mut self) {
        if self.available_printers.is_empty() {
            println!("No printers found");
            return;
        }

        println!("Select a printer:");
        for (i, printer) in self.available_printers.iter().enumerate() {
            println!("{}: {}", i, printer.address);
        }
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let index = input.trim().parse::<usize>().unwrap();
        let message = "<M115>\n";
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
    address: String,
    body: String,
    status: ResponseStatus,
}

impl Response {
    fn from_data(address: String, data: &str) -> Self {
        let trimmed = data.trim();
        let status = if trimmed.ends_with("ok") {
            ResponseStatus::Ok
        } else {
            ResponseStatus::Error
        };

        let body = trimmed[..trimmed.rfind('\n').unwrap_or(trimmed.len())].to_string();

        Self {
            address,
            body,
            status,
        }
    }
}
