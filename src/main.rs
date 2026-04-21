use std::net::{TcpListener, TcpStream};
use std::result;
use std::io::{ Write, Read };
use std::fmt::{Display, Formatter};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::sync::Arc;
use std::collections::HashMap;
type Result<T> = result::Result<T, ()>;
const SAFE_MODE: bool = true;

struct Sensitive<T>{
    inner: T,
}

impl<T: Display> Display for Sensitive<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if SAFE_MODE {
            writeln!(f, "[REDACTED]")
        } else {
            writeln!(f, "{}", self.inner)
        }
    }
}

enum Message {
    ClientConnected{author: Arc<TcpStream>},
    ClientDisconnected{author: Arc<TcpStream>},
    NewMessage{author: Arc<TcpStream>, buffer: Vec<u8>},
}

struct Client {
    conn: Arc<TcpStream>,
}

fn client(stream: Arc<TcpStream>, messages: Sender<Message>) -> Result<()> {
    messages.send(Message::ClientConnected{author: stream.clone()}).map_err(|e| {
        eprintln!("ERROR: couldn't write to a stream: {e}");
    })?;
    let mut buffer = [0u8; 1024]; //fixed-size buffer to prevent memory exhaustion
    loop {
       let n = stream.as_ref().read(&mut buffer).map_err(|e| {
           eprintln!("ERROR: couldn't read from stream: {e}");
           let _ = messages.send(Message::ClientDisconnected{author: stream.clone()});
       })?;

       if n == 0 { // Clean disconnect
        let _ = messages.send(Message::ClientDisconnected{author: stream.clone()});
        break;
       }

        messages.send(Message::NewMessage{author: stream.clone(), buffer: buffer[0..n].to_vec()}).map_err(|e| {
            eprintln!("ERROR: couldn't send to server: {e}");
        })?;
    }
    Ok(())
}

fn server(messages: Receiver<Message>) {
   let mut clients = HashMap::new();
    loop {
        let msg = messages.recv().expect("server couldn't receive");
        match msg {
            Message::ClientConnected { author: stream } => {
                if let Ok(addr) = stream.peer_addr() { //Use "if let" instead of unwrap to hable errors
                    clients.insert(addr, Client { conn: stream });
                    println!("INFO: Client connected from {}", addr); 
                }
            }
            Message::ClientDisconnected { author: stream } => {
                if let Ok(addr) = stream.peer_addr() { //Same here too
                    clients.remove(&addr);
                    println!("INFO: Client disconnected from {}", addr);
                }  
            }
            Message::NewMessage { author: stream, buffer: byte } => {
                if let Ok(author_addr) = stream.peer_addr() {
                    let text = String::from_utf8_lossy(&byte); //Sanitization to check if bytes are valid UTF-8
                    let mut msg = format!("{}> ", author_addr).into_bytes(); //To avoid terminal injection
                    msg.extend(text.as_bytes());

                    for (addr, client) in clients.iter() {
                        if *addr != author_addr {
                            let _ = client.conn.as_ref().write_all(&msg); //Ignore errors on write when a client dies and doesn't stop the loop
                        }
                    }
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let address = "0.0.0.0:6969"; // To alllow connection from outside your own machine
    println!("Listening for requests at http://{}", Sensitive{inner: address});
    let listener = TcpListener::bind(address).map_err(|e| {
        eprintln!("ERROR: could not bind {address}:{e}", e = Sensitive{inner: e})
    })?;

    let (message_sender, message_receiver) = channel();
    thread::spawn(|| server(message_receiver));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let message_sender = message_sender.clone();
                let stream = Arc::new(stream);
                thread::spawn(|| client(stream, message_sender));
            }
            Err(e) => {
                eprintln!("ERROR: could not accept connection {address}:{e}");
            }
        }
    }
    Ok(())
}
