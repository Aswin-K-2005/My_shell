use bstr::ByteSlice;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::str;

fn file_cleanup() {
    let path = Path::new("/tmp/aish_memory.sock");
    if path.exists() {
        match fs::remove_file(path) {
            Ok(()) => println!("file deleted successfully!"),
            Err(e) => println!("Failed to delete: {}", e),
        }
    }
}

enum Command {
    SaveMemory,
    Retrieve,
}

enum ParseError {
    UnknownCommand,
}

// Fixed: Changed name to SCREAMING_SNAKE_CASE to clear compiler warnings
static mut GLOBAL_ID: u64 = 1;

#[derive(Debug)]
struct Memory {
    id: u64,
    project: String,
    entity: String,
    content: String,
}

impl Memory {
    fn new(project: String, entity: String, content: String) -> Self {
        let unique_id = unsafe {
            let current = GLOBAL_ID;
            GLOBAL_ID += 1;
            current
        };

        Self {
            id: unique_id,
            project,
            entity,
            content,
        }
    }
}

enum ValidationError {
    MissingProject,
    MissingEntity,
    MissingContent,
}

fn build_memory(mut fields: HashMap<String, String>) -> Result<Memory, ValidationError> {
    let project = match fields.remove("project") {
        Some(val) => val,
        None => return Err(ValidationError::MissingProject),
    };
    let entity = match fields.remove("entity") {
        Some(val) => val,
        None => return Err(ValidationError::MissingEntity),
    };
    let content = match fields.remove("content") {
        Some(val) => val,
        None => return Err(ValidationError::MissingContent),
    };

    Ok(Memory::new(project, entity, content))
}

fn parser(message: &str) -> Result<Command, ParseError> {
    match message.trim() {
        "SAVE_MEMORY" => Ok(Command::SaveMemory),
        "RETRIEVE" => Ok(Command::Retrieve),
        _ => Err(ParseError::UnknownCommand),
    }
}

fn parser_field(message: Vec<&str>) -> HashMap<String, String> {
    let mut argument_map = HashMap::new();
    for item in message {
        let mut parts = item.splitn(2, '=');
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            argument_map.insert(key.to_string(), value.to_string());
        }
    }
    argument_map
}

fn process_client_request(message_str: &str) {
    println!("Received Message : {}", message_str);

    let mut parts = message_str.split('|');
    if let Some(command) = parts.next() {
        match parser(command) {
            Ok(Command::SaveMemory) => println!("SaveMemory"),
            Ok(Command::Retrieve) => println!("Retrieve"),
            Err(ParseError::UnknownCommand) => {
                println!("UnknownCommand");
                return;
            }
        }
    }

    let remaining_msg: Vec<&str> = parts.collect();
    let argument_map = parser_field(remaining_msg);
    println!("{:?}", argument_map);

    let memory_struct = match build_memory(argument_map) {
        Ok(memory) => memory,
        Err(ValidationError::MissingProject) => {
            println!("Missing Project !");
            return;
        }
        Err(ValidationError::MissingEntity) => {
            println!("Missing Entity !");
            return;
        }
        Err(ValidationError::MissingContent) => {
            println!("Missing Content !");
            return;
        }
    };

    println!("Successfully processed memory: {:#?}", memory_struct); // Using {:#?} for pretty printing!
}

fn main() -> std::io::Result<()> {
    file_cleanup();

    let socket_path = Path::new("/tmp/aish_memory.sock");
    let listener = UnixListener::bind(socket_path)?;

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("connected successfully");

                let mut buffer = Vec::new();
                let mut temp = [0; 256];
                let delimiter = b"__MSG_END__";

                // Optimization: The loop returns the clean String directly out of the break statement.
                // No Option or if-let unwrapping required out here anymore!
                let raw_message_string = loop {
                    let bytes_read = stream.read(&mut temp)?;

                    if bytes_read == 0 {
                        println!("Client disconnected without delimiter");
                        break String::new(); // Return an empty string if client hung up
                    }

                    buffer.extend_from_slice(&temp[..bytes_read]);

                    if let Some(pos) = buffer.find(delimiter) {
                        let message = &buffer[..pos];
                        let message_str = str::from_utf8(message).expect("not str");

                        println!("Recieved Message : {}", message_str);

                        break message_str.to_string(); // Passes value directly out of loop
                    }
                };

                // If we got a valid message, pass it to the handler!
                if !raw_message_string.is_empty() {
                    process_client_request(&raw_message_string);
                }
            }
            Err(err) => {
                println!("Failed to connect: {}", err);
            }
        }
    }

    Ok(())
}

