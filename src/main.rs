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
    let mut buffer = Vec::new();
    buffer.resize(64, 0);
    loop {
       let n = stream.as_ref().read(&mut buffer).map_err(|e| {
           eprintln!("ERROR: couldn't read from stream: {e}");
           let _ = messages.send(Message::ClientDisconnected{author: stream.clone()});
       })?;
        messages.send(Message::NewMessage{author: stream.clone(), buffer: buffer[0..n].to_vec()}).map_err(|e| {
            eprintln!("ERROR: couldn't send to server: {e}");
        })?;
    }
}

fn server(messages: Receiver<Message>) {
   let mut clients = HashMap::new();
    loop {
        let msg = messages.recv().expect("server couldn't receive");
        match msg {
            Message::ClientConnected { author: stream } => {
                let addr = stream.peer_addr().unwrap();
                clients.insert(addr, Client { conn: stream });
            }
            Message::ClientDisconnected { author: stream } => {
                let addr = stream.peer_addr().unwrap();
                clients.remove(&addr);
            }
            Message::NewMessage { author: stream, buffer: byte } => {
                let author_addr = stream.peer_addr().unwrap();
                let mut msg = format!("{}> ", author_addr).into_bytes();
                msg.extend(&byte);

                for (addr, client) in clients.iter() {
                    if *addr != author_addr {
                        let _ = client.conn.as_ref().write(&msg);
                    }
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let address = "127.0.0.1:6969";
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
