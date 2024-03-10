use std::{collections::HashMap, io::{Read, Write}, marker::PhantomData, net::{SocketAddr, TcpListener, TcpStream}, os::windows::io::AsSocket, sync::{Arc, Mutex, RwLock}, thread::JoinHandle, time::Duration};

use crate::{protocol::{NetworkQueue, Packet}, AnyError};

pub struct Server<C: Packet, S: Packet> {
    clients: Arc<RwLock<HashMap<SocketAddr, Arc<ServerClient<C, S>>>>>,
    acceptor: Option<JoinHandle<()>>,
    duty: Option<JoinHandle<()>>,
    running: Arc<RwLock<bool>>,
    events: Arc<Mutex<Vec<ServerEvent<C>>>>
}

impl<C: Packet + 'static, S: Packet + 'static> Server<C, S> {
    pub fn start(server_address: SocketAddr) -> Result<Self, AnyError> {
        let listener = TcpListener::bind(server_address)?;
        
        let running = Arc::new(RwLock::new(true));
        let acceptor_running = running.clone();
        let duty_running = running.clone();
        let clients = Arc::new(RwLock::new(HashMap::new()));
        let clients_duty = clients.clone();
        let acceptor_clients = clients.clone();
        let events = Arc::new(Mutex::new(vec![]));
        let events_clients = events.clone();

        Ok(Self {
            clients,
            acceptor: Some(std::thread::spawn(move || Self::acceptor_thread(listener, acceptor_running, acceptor_clients, events_clients))),
            duty: Some(std::thread::spawn(move || Self::dudy_thread(duty_running, clients_duty))),
            running,
            events
        })
    }

    fn acceptor_thread(listener: TcpListener, running: Arc<RwLock<bool>>, clients: Arc<RwLock<HashMap<SocketAddr, Arc<ServerClient<C, S>>>>>, events: Arc<Mutex<Vec<ServerEvent<C>>>>) {
        while *running.read().unwrap() {
            let (stream, addr) = listener.accept().expect("Unable to accept client connection");
            let client = ServerClient::new(addr, stream, events.clone()).expect("Unable to create client");
            clients.write().unwrap().insert(addr, Arc::new(client));
            events.lock().unwrap().push(ServerEvent::Connect(addr));
        }
    }

    fn dudy_thread(running: Arc<RwLock<bool>>, clients: Arc<RwLock<HashMap<SocketAddr, Arc<ServerClient<C, S>>>>>) {
        while *running.read().unwrap() {
            clients.write().unwrap().retain(|_, c| {
                if c.running() {
                    true
                } else {
                    c.disconnect();
                    false
                }
            });
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn events(&self) -> impl Iterator<Item=ServerEvent<C>> {
        std::mem::take(&mut *self.events.lock().unwrap()).into_iter()
    }

    pub fn client(&self, address: SocketAddr) -> Option<Arc<ServerClient<C, S>>> {
        self.clients.read().unwrap().get(&address).cloned()
    }
}

impl<C: Packet, S: Packet> Drop for Server<C, S> {
    fn drop(&mut self) {
        *self.running.write().unwrap() = false;
        for client in self.clients.read().unwrap().values() {
            *client.connected.write().unwrap() = false;
        }
        for client in self.clients.write().unwrap().values_mut() {
            let _ = client.read_thread.lock().unwrap().take().unwrap().join();
            let _ = client.write_thread.lock().unwrap().take().unwrap().join();
        }
        let _ = self.acceptor.take().unwrap().join();
        let _ = self.duty.take().unwrap().join();
    }
}

#[derive(Debug)]
pub enum ServerEvent<P: Packet> {
    Connect(SocketAddr),
    Message(SocketAddr, P),
    Disconnect(SocketAddr)
}

pub struct ServerClient<C: Packet, S: Packet> {
    address: SocketAddr,
    read_thread: Mutex<Option<JoinHandle<()>>>,
    write_thread: Mutex<Option<JoinHandle<()>>>,
    send: Arc<Mutex<Vec<S>>>,
    events: Arc<Mutex<Vec<ServerEvent<C>>>>,
    connected: Arc<RwLock<bool>>,
}

impl<C: Packet + 'static, S: Packet + 'static> ServerClient<C, S> {
    fn new(address: SocketAddr, stream: TcpStream, events: Arc<Mutex<Vec<ServerEvent<C>>>>) -> Result<Self, AnyError> {
        let reader = stream.try_clone()?;
        let writer = stream;
        let connected = Arc::new(RwLock::new(true));
        let connected_read = connected.clone();
        let connected_write = connected.clone();
        let send = Arc::new(Mutex::new(Vec::new()));
        let read_send = send.clone();
        let events_send = events.clone();
        let events_receive = events.clone();
        Ok(Self {
            address,
            read_thread: Mutex::new(Some(std::thread::spawn(move||Self::read_thread(address, events_receive, connected_read, reader)))),
            write_thread: Mutex::new(Some(std::thread::spawn(move||Self::write_thread(read_send, address, events_send, connected_write, writer)))),
            send,
            events,
            connected
        })
    }
    pub fn send(&self, packet: S) {
        self.send.lock().unwrap().push(packet);
    }

    pub fn disconnect(&self) {
        let mut connected = self.connected.write().unwrap();
        if *connected {
            *connected = false;
            self.events.lock().unwrap().push(ServerEvent::Disconnect(self.address));
        }
    }

    pub fn running(&self) -> bool {
        *self.connected.read().unwrap()
    }

    fn read_thread(address: SocketAddr, events: Arc<Mutex<Vec<ServerEvent<C>>>>, connected: Arc<RwLock<bool>>, mut reader: TcpStream) {
        let _: Result<(), AnyError> = try {
            while *connected.read().unwrap() {
                let mut len = [0;4];
                reader.read_exact(&mut len[..])?;
                let mut buffer = vec![0;u32::from_le_bytes(len) as usize];
                reader.read_exact(&mut buffer[..])?;
                let packet = C::deserialize(&buffer[..])?;
                events.lock().unwrap().push(ServerEvent::Message(address, packet));
            }
        };
        reader.shutdown(std::net::Shutdown::Both).expect("could not shut down read connection");
        let mut connected = connected.write().unwrap();
        if *connected {
            *connected = false;
            events.lock().unwrap().push(ServerEvent::Disconnect(address));
        }
    }

    fn write_thread(send: Arc<Mutex<Vec<S>>>, address: SocketAddr, events: Arc<Mutex<Vec<ServerEvent<C>>>>, connected: Arc<RwLock<bool>>, mut writer: TcpStream) {
        let _: Result<(), AnyError> = try {
            while *connected.read().unwrap() {
                for packet in std::mem::take(&mut *send.lock().unwrap()) {
                    let mut buffer = vec![0;4];
                    packet.serialize(&mut buffer);
                    let len = buffer.len() as u32 - 4;
                    buffer[..4].copy_from_slice(&len.to_le_bytes()[..]);
                    writer.write_all(&buffer)?;
                }
            }
        };
        writer.shutdown(std::net::Shutdown::Both).expect("could not shut down write connection");
        let mut connected = connected.write().unwrap();
        if *connected {
            *connected = false;
            events.lock().unwrap().push(ServerEvent::Disconnect(address));
        }
    }
}
