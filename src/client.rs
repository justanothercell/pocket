use std::{io::{Read, Write}, net::{SocketAddr, TcpStream}, sync::{Arc, RwLock}, thread::JoinHandle};

use crate::{protocol::{NetworkQueue, Packet}, AnyError};

pub struct Client<C: Packet, S: Packet> {
    read_thread: Option<JoinHandle<()>>,
    write_thread: Option<JoinHandle<()>>,
    queue: Arc<NetworkQueue<C, S>>,
    running: Arc<RwLock<bool>>
}

impl<C: Packet + 'static, S: Packet + 'static> Client<C, S> {
    pub fn start(server_address: SocketAddr) -> Result<Self, AnyError> {
        let stream = TcpStream::connect(server_address)?;
        let reader = stream.try_clone()?;
        let writer = stream;
        let queue = Arc::new(NetworkQueue::new());
        let read_queue = queue.clone();
        let write_queue = queue.clone();
        let running = Arc::new(RwLock::new(true));
        let read_running = running.clone();
        let write_running = running.clone();
        Ok(Self {
            read_thread: Some(std::thread::spawn(move||Self::read_thread(read_queue, read_running, reader))),
            write_thread: Some(std::thread::spawn(move||Self::write_thread(write_queue, write_running, writer))),
            queue,
            running
        })
    }

    pub fn send(&self, packet: C) {
        self.queue.add_sendable(packet);
    }

    pub fn received(&self) -> impl Iterator<Item=S> {
        self.queue.take_received().into_iter()
    }

    fn read_thread(queue: Arc<NetworkQueue<C, S>>, running: Arc<RwLock<bool>>, mut reader: TcpStream) {
        let _: Result<(), AnyError> = try {
            while *running.read().unwrap() {
                let mut len = [0;4];
                reader.read_exact(&mut len[..])?;
                let mut buffer = vec![0;u32::from_le_bytes(len) as usize];
                reader.read_exact(&mut buffer[..])?;
                let packet = S::deserialize(&buffer[..])?;
                queue.add_received(packet);
            }
        };
        reader.shutdown(std::net::Shutdown::Both).expect("could not shut down read connection");
        *running.write().unwrap() = false;
    }

    fn write_thread(queue: Arc<NetworkQueue<C, S>>, running: Arc<RwLock<bool>>, mut writer: TcpStream) {
        let _: Result<(), AnyError> = try {
            while *running.read().unwrap() {
                for packet in queue.take_sendable() {
                    let mut buffer = vec![0;4];
                    packet.serialize(&mut buffer);
                    let len = buffer.len() as u32 - 4;
                    buffer[..4].copy_from_slice(&len.to_le_bytes()[..]);
                    writer.write_all(&buffer)?;
                }
            }
            writer.shutdown(std::net::Shutdown::Both)?;
        };
        writer.shutdown(std::net::Shutdown::Both).expect("could not shut down write connection");
        *running.write().unwrap() = false;
    }

    pub fn running(&self) -> bool {
        *self.running.write().unwrap()
    }

    pub fn disconnect(self) {
        // drop
    }
}

impl<C: Packet, S: Packet> Drop for Client<C, S> {
    fn drop(&mut self) {
        *self.running.write().unwrap() = false;
        let _ = self.read_thread.take().unwrap().join();
        let _ = self.write_thread.take().unwrap().join();
    }
}