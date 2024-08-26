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

    fn search(&mut self) -> std::io::Result<Vec<Printer>> {
        let ip_parts = self.local_ip.split('.').collect::<Vec<_>>();

        let data = format!(
            "<M888 A{} B{} C{} D{} P{}>\n",
            ip_parts[0], ip_parts[1], ip_parts[2], ip_parts[3], self.port
        );

        self.socket.set_broadcast(true)?;
        self.socket.send_to(data.as_bytes(), &self.broadcast)?;
        println!("Sent: {}", data);

        let mut found_printers = Vec::new();

        let mut buf = [0; 1024];

        println!("Searching for printers...");

        let (number_of_bytes, src_addr) = self.socket.recv_from(&mut buf)?;
        let received_data = String::from_utf8_lossy(&buf[..number_of_bytes]);

        let response = Response::from_data(&received_data);

        println!(
            "Received message from {}: {}\n Status: {:}",
            src_addr, response.body, response.status
        );

        if response.status == ResponseStatus::Ok {
            let printer = Printer {
                info: HashMap::new(),
                address: src_addr.to_string(),
            };

            found_printers.push(printer);
        }

        Ok(found_printers)
    }

    fn get_printer_info(&self, printer: &Printer) -> std::io::Result<Printer> {
        let message = "<M115>\n";
        self.socket.send_to(message.as_bytes(), &printer.address)?;

        let mut buf = [0; 1024];

        let (number_of_bytes, src_addr) = self.socket.recv_from(&mut buf)?;
        let received_data = String::from_utf8_lossy(&buf[..number_of_bytes]);

        if src_addr.to_string() == printer.address && !received_data.trim().is_empty() {
            let response = Response::from_data(&received_data);
            let mut printer = Printer {
                info: HashMap::new(),
                address: src_addr.to_string(),
            };
            printer.info = response
                .body
                .split('\r')
                .filter(|s| !s.trim().is_empty())
                .map(|s| {
                    let cleaned = s.replace("\n", "");
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
            return Ok(printer);
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to get printer info",
        ))
    }

    fn get_stats(&self, printer: &Printer) -> std::io::Result<String> {
        let message = "M86\n";
        self.socket.send_to(message.as_bytes(), &printer.address)?;

        let mut buf = [0; 1024];

        let (number_of_bytes, src_addr) = self.socket.recv_from(&mut buf)?;
        let received_data = String::from_utf8_lossy(&buf[..number_of_bytes]);

        if src_addr.to_string() == printer.address && !received_data.trim().is_empty() {
            let response = Response::from_data(&received_data);
            println!("{}", response.body);
            return Ok(response.body);
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to get printer stats",
        ))
    }
}

#[derive(Debug)]
enum SelectedOption {
    Search,
    SelectPrinter,
    // ConnectManually,
    Exit,
    None,
}

impl Default for SelectedOption {
    fn default() -> Self {
        Self::Search
    }
}

#[derive(Debug, Clone)]
struct Printer {
    info: HashMap<String, String>,
    address: String,
}

struct InterfaceError {
    message: String,
}

#[derive(Debug)]
struct PrinterInterface<'a> {
    server: &'a mut Server,
}

impl<'a> PrinterInterface<'a> {
    fn new(server: &'a mut Server) -> Self {
        Self { server }
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

            let selected_option = match index {
                1 => SelectedOption::Search,
                2 => SelectedOption::SelectPrinter,
                // 4 => SelectedOption::ConnectManually,
                0 => SelectedOption::Exit,
                _ => SelectedOption::None,
            };

            match selected_option {
                SelectedOption::Search => self.search(),
                SelectedOption::SelectPrinter => self.select_printer(),
                // SelectedOption::ShowStats => self.show_stats(),
                // SelectedOption::ConnectManually => self.connect_manually(),
                SelectedOption::Exit => break,
                SelectedOption::None => println!("Invalid option"),
            }
        }
    }

    fn search(&mut self) {
        println!("Searching for printers...");
        self.available_printers = self.server.search().unwrap_or_else(|err| {
            println!("Error: {}", err);
            Vec::new()
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
        self.selected_printer = self
            .server
            .get_printer_info(&self.available_printers[index])
            .map_or_else(
                |_| {
                    println!("Failed to get printer info");
                    None
                },
                |printer| {
                    println!("Selected printer: {}", printer.address);
                    println!("Info:");
                    for (key, value) in printer.info.iter() {
                        println!("{}: {}", key, value);
                    }
                    Some(printer)
                },
            );
    }

    fn show_stats(&self) {
        if let Some(printer) = &self.selected_printer {
            self.server.get_stats(printer).map_or_else(
                |_| println!("Failed to get stats"),
                |stats| println!("Stats: {}", stats),
            );
        } else {
            println!("No printer selected");
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
