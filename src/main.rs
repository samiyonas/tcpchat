use std::net::{SocketAddr, TcpListener, TcpStream}; use std::result;
use std::io::{ Write, Read };
use std::fmt::{ Display, Formatter };
use std::sync::mpsc::{ channel, Sender, Receiver };
use std::thread;
use std::sync::{ Arc, Mutex };
use std::collections::HashMap;
use rustls::{ StreamOwned, ServerConfig, ServerConnection };
use rustls_pemfile;
use std::fs::File;
use std::io::BufReader;

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
    ClientConnected{author: SocketAddr, stream: Arc<Mutex<StreamOwned<ServerConnection, TcpStream>>>},
    ClientDisconnected{author: SocketAddr},
    NewMessage{author: SocketAddr, buffer: Vec<u8>},
}

struct Client {
    conn: Arc<Mutex<StreamOwned<ServerConnection, TcpStream>>>,
}

fn client(stream: StreamOwned<ServerConnection, TcpStream>, messages: Sender<Message>, addr: SocketAddr) -> Result<()> {
    let shared_stream = Arc::new(Mutex::new(stream));
    messages.send(Message::ClientConnected{author: addr, stream: shared_stream.clone()}).map_err(|e| {
        eprintln!("ERROR: couldn't write to a stream: {e}");
    })?;
    let mut buffer = Vec::new();
    buffer.resize(64, 0);
    loop {
        let mut conn = shared_stream.lock().unwrap();
       let n = conn.read(&mut buffer).map_err(|e| {
           eprintln!("ERROR: couldn't read from stream: {e}");
           let _ = messages.send(Message::ClientDisconnected{author: addr});
       })?;
        messages.send(Message::NewMessage{author: addr, buffer: buffer[0..n].to_vec()}).map_err(|e| {
            eprintln!("ERROR: couldn't send to server: {e}");
        })?;
    }
}

fn server(messages: Receiver<Message>) {
   let mut clients = HashMap::new();
    loop {
        let msg = messages.recv().expect("server couldn't receive");
        match msg {
            Message::ClientConnected { author: addr, stream} => {
                clients.insert(addr, Client { conn: stream.clone() });
            }
            Message::ClientDisconnected { author: addr } => {
                clients.remove(&addr);
            }
            Message::NewMessage { author: addr, buffer: byte } => {
                let mut msg = format!("{}> ", addr).into_bytes();
                msg.extend(&byte);

                for (address, client) in clients.iter() {
                    if *address != addr {
                        let mut cli_conn = client.conn.lock().unwrap();
                        let _ = cli_conn.write(&msg);
                    }
                }
            }
        }
    }
}

fn main() -> Result<()> {
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
