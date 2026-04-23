use std::net::{SocketAddr, TcpListener, TcpStream};
use std::result;
use std::io::{ Write, Read };
use std::fmt::{ Display, Formatter };
use std::sync::mpsc::{ channel, Sender, Receiver };
use std::thread;
use std::sync::{ Arc };
use std::collections::HashMap;
use rustls::{StreamOwned, ServerConfig, ServerConnection};
use rustls_pemfile;
use std::fs::File;
use std::io::BufReader;
use env_logger;

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
     ClientConnected{author: SocketAddr, sender: Sender<Message>},
     ClientDisconnected{author: SocketAddr},
    NewMessage{author: SocketAddr, buffer: Vec<u8>},
}

struct Client {
    conn: Sender<Message>,
}

fn client(mut stream: StreamOwned<ServerConnection, TcpStream>, messages: Sender<Message>, addr: SocketAddr)  {
    let (cs, cr) = channel();
    let _ = messages.send(Message::ClientConnected{author: addr, sender: cs}).map_err(|e| {
        eprintln!("ERROR: couldn't write to a stream: {e}");
    });
    let mut buffer = Vec::new();
    buffer.resize(1024, 0);
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                let _ = messages.send(Message::ClientDisconnected{author: addr});
                break
            }
            Ok(n) => {
                let _ = messages.send(Message::NewMessage{author: addr, buffer: buffer[0..n].to_vec()});
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_e) => {
                let _ = messages.send(Message::ClientDisconnected{author: addr});
            },
        }
        while let Ok(msg) = cr.try_recv() {
            match msg {
                Message::NewMessage{author: _author,  buffer} => {
                    let _ = stream.write_all(&buffer);
                    let _ = stream.flush();
                }
                _ => {}
            }
        }
    }
}

fn server(messages: Receiver<Message>) {
    let mut clients = HashMap::new();
    loop {
        let msg = messages.recv().expect("server couldn't receive");
        match msg {
            Message::ClientConnected{author, sender} => {
                clients.insert(author, Client {conn: sender});
            }
            Message::ClientDisconnected{author} => {
                clients.remove(&author);
            }
            Message::NewMessage{author, buffer} => {
                let mut dead_clients = Vec::new();
                let mut text = format!("{}> ", author).into_bytes();
                text.extend(buffer);
                for (addr, client) in clients.iter() {
                    if *addr != author {
                        if  client.conn.send(Message::NewMessage{author, buffer: text.clone()}).is_err() {
                            dead_clients.push(addr.clone());
                        }
                    }
                }

                for dead in dead_clients {
                    clients.remove(&dead);
                }
            }
        }

    }
}

fn main() -> Result<()> {
    env_logger::init();
    let cert_file = File::open("cert.pem").unwrap();
    let mut reader = BufReader::new(cert_file);
    let certs = rustls_pemfile::certs(&mut reader);
    let mut certificate = Vec::new();
    for cert in certs {
        match cert {
            Ok(cert) => {certificate.push(cert);},
            Err(_) => {return Err(());}
        }
    }

    let key_file = File::open("key.pem").unwrap();
    let mut reader = BufReader::new(key_file);
    let secret_key;
    match rustls_pemfile::private_key(&mut reader) {
        Ok(Some(key)) => { secret_key = key; },
        Ok(None) => { return Err(()); },
        Err(_) => { return Err(()); }
    }

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificate, secret_key)
        .expect("could not build server config");

    let address = "0.0.0.0:6969";
    println!("Listening for requests at http://{}", Sensitive{inner: address});
    let listener = TcpListener::bind(address).map_err(|e| {
        eprintln!("ERROR: could not bind {address}:{e}", e = Sensitive{inner: e})
    })?;

    let (message_sender, message_receiver) = channel();
    thread::spawn(|| server(message_receiver));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                stream.set_nonblocking(true).map_err(|err| {
                    eprintln!("ERROR: could not set nonblocking on {address}:{err}");
                })?;
                let addr = stream.peer_addr().unwrap();
                let message_sender = message_sender.clone();
                let server_connection = ServerConnection::new(Arc::new(server_config.clone())).unwrap();
                let tls_stream = StreamOwned::new(server_connection, stream);
                thread::spawn(move || client(tls_stream, message_sender, addr));
            }
            Err(e) => {
                eprintln!("ERROR: could not accept connection {address}:{e}");
            }
        }
    }
    Ok(())
}
