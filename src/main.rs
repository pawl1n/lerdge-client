use std::collections::HashMap;

use server::{ResponseStatus, UdpServer};
mod server;

const PORT: u16 = 0; // Port to listen on
const BROADCAST: &str = "255.255.255.255"; // Broadcast address
const TARGET_PORT: u16 = 8686; // Target printer port

fn main() {
    let broadcast = format!("{}:{}", BROADCAST, TARGET_PORT);
    let local_ip = local_ip_address::local_ip().expect("Could not get local IP address");
    let server = server::UdpServer::new(local_ip.to_string(), PORT, broadcast);
    let mut main_interface = MainInterface {
        server,
        ..Default::default()
    };

    main_interface.run();
}

#[derive(Debug, Clone)]
struct Printer {
    info: HashMap<String, String>,
    address: String,
}

#[derive(Debug)]
struct PrinterInterface<'a> {
    server: &'a mut UdpServer,
    printer: Printer,
}

impl<'a> PrinterInterface<'a> {
    fn new(server: &'a mut UdpServer, printer: Printer) -> Self {
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
        let filesize = std::fs::metadata(file_path).expect("File not found").len();

        let message = format!("M828 P{filesize} U:{filename}\n\n"); // Create file

        self.server
            .send_message(&message, &self.printer.address)
            .map_or_else(
                |_| println!("Failed to create file"),
                |response| println!("File created: {}", response.body),
            );

        let chunk_size = 1442; // Chunk size in bytes (1450 - 8 bytes header = 1442 bytes)
        let mut start = 0;

        use std::io::{Read, Seek, SeekFrom};

        let mut f = std::fs::File::open(file_path).expect("Failed to open file");

        // Wait for ok message
        loop {
            let response = self.server.read_message();
            if let Ok(response) = response {
                println!("{}", response.body);
                if response.body.starts_with("ok") {
                    break;
                }
            }
        }

        while start < filesize {
            let chunk_number;
            let response = self.server.read_message();
            if let Ok(response) = response {
                match parse_chunk_number(&response.body) {
                    ChunkParseResult::Chunk(chunk) => chunk_number = chunk,
                    ChunkParseResult::Error(error) => {
                        eprintln!("\nFailed to parse chunk: {error}");
                        break;
                    }
                    ChunkParseResult::FileTransferCompleted => {
                        println!("\nFile transfer completed");
                        break;
                    }
                    ChunkParseResult::FileTransferError => {
                        println!("\nFile transfer error");
                        break;
                    }
                }

                if chunk_number * chunk_size > filesize {
                    continue;
                }
            } else {
                eprintln!("\n{}", response.unwrap_err());
                continue;
            }

            start = chunk_number * chunk_size;
            let count = std::cmp::min(chunk_size, filesize - start) as usize;
            print!(
                "\rSending chunk {}: {}-{} of {}",
                chunk_number,
                start,
                start + count as u64,
                filesize
            );
            use std::io::{stdout, Write};
            stdout().flush().unwrap();

            _ = f.seek(SeekFrom::Start(start)).expect("Failed to seek");
            let mut buf = vec![0; count];
            f.read_exact(&mut buf).expect("Failed to read file");

            let mut data = vec![0_u8; 8];

            let mut offset = 0;
            data[offset] = 0xaa;
            offset += 1;

            if chunk_number > (u8::MAX as u64 + 1).pow(3) {
                eprint!("\nChunk number is too large: {}", chunk_number);
                return;
            }

            data[offset] = (chunk_number >> 16 & 0xff) as u8;
            offset += 1;
            data[offset] = (chunk_number >> 8 & 0xff) as u8;
            offset += 1;
            data[offset] = (chunk_number & 0xff) as u8;
            offset += 1;

            if count > u16::MAX as usize {
                eprint!("Length is too large: {}", count);
                return;
            }

            data[offset] = (count >> 8 & 0xff) as u8;
            offset += 1;
            data[offset] = (count & 0xff) as u8;
            offset += 1;

            data[offset] = xor8checksum(&buf[..count]);

            offset += 1;

            data[offset] = 0x55;
            data.extend_from_slice(&buf[..count]);

            let response = self.server.send_bytes(&data, &self.printer.address);

            if let Err(error) = response {
                eprintln!("Failed to send data: {:?}", error);
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum ChunkParseResult {
    Chunk(u64),
    FileTransferCompleted,
    FileTransferError,
    Error(String),
}

fn parse_chunk_number(data: &str) -> ChunkParseResult {
    let re = regex::Regex::new(r"N (?<chunk>[0-9]+)").expect("Invalid regex");

    if data.contains("File transfer completed") {
        return ChunkParseResult::FileTransferCompleted;
    } else if data.contains("File transfer error") {
        return ChunkParseResult::FileTransferError;
    } else if let Some(captures) = re.captures(data) {
        let chunk_number = captures["chunk"].trim().parse::<u64>();

        return match chunk_number {
            Ok(chunk_number) => ChunkParseResult::Chunk(chunk_number),
            Err(error) => ChunkParseResult::Error(error.to_string()),
        };
    }

    ChunkParseResult::Error("Failed to parse chunk".to_string())
}

fn xor8checksum(data: &[u8]) -> u8 {
    if data.len() <= 8 {
        return 0x00;
    }

    data.iter()
        .take(data.len() - 8)
        .fold(0_u8, |acc, x| acc ^ x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xor8checksum6() {
        let data = "Test\r\n".as_bytes();
        let checksum = xor8checksum(data);
        assert_eq!(checksum, 0x00);
    }

    #[test]
    fn test_xor8checksum7() {
        let data = "Testq\r\n".as_bytes();
        let checksum = xor8checksum(data);
        assert_eq!(checksum, 0x00);
    }

    #[test]
    fn test_xor8checksum8() {
        let data = "Testqw\r\n".as_bytes();
        let checksum = xor8checksum(data);
        assert_eq!(checksum, 0x00);
    }

    #[test]
    fn test_xor8checksum9() {
        let data = "Testqwe\r\n".as_bytes();
        let checksum = xor8checksum(data);
        assert_eq!(checksum, 0x54);
    }

    #[test]
    fn test_xor8checksum10() {
        let data = "Testqwer\r\n".as_bytes();
        let checksum = xor8checksum(data);
        assert_eq!(checksum, 0x31);
    }

    #[test]
    fn test_xor8checksum11() {
        let data = "Testqwert\r\n".as_bytes();
        let checksum = xor8checksum(data);
        assert_eq!(checksum, 0x42);
    }

    #[test]
    fn test_xor8checksum12() {
        let data = "Testqwerty\r\n".as_bytes();
        let checksum = xor8checksum(data);
        assert_eq!(checksum, 0x36);
    }

    #[test]
    fn test_parse_chunk_number() {
        let data = "N 1234 ok\r\n";
        let result = parse_chunk_number(data);
        assert_eq!(result, ChunkParseResult::Chunk(1234));
    }
}

#[derive(Debug, Default)]
struct MainInterface {
    server: server::UdpServer,
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
        let response = self.server.send_broadcast_message(&data);

        match response {
            Ok(response) => match response.status {
                ResponseStatus::Ok => {
                    self.available_printers.push(Printer {
                        address: response.address,
                        info: Default::default(),
                    });
                }
                _ => {
                    println!("Error searching for printers: {}", response.body);
                }
            },
            Err(e) => {
                eprintln!("Error searching for printers: {}", e);
            }
        }
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
