use std::{collections::VecDeque, sync::Mutex};

use nanoserde::{DeBin, SerBin};

use crate::AnyError;

pub trait Packet: Send + Sync {
    fn serialize(&self, buffer: &mut Vec<u8>);

    fn deserialize(buffer: &[u8]) -> Result<Self, AnyError> where Self: Sized;
}

impl<T: SerBin + DeBin + Send + Sync> Packet for T {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        self.ser_bin(buffer);
    }

    fn deserialize(buffer: &[u8]) -> Result<Self, AnyError> {
        Ok(Self::de_bin(&mut 0, buffer)?)
    }
}

pub(crate) struct NetworkQueue<S: Packet, R: Packet> {
    send: Mutex<VecDeque<S>>,
    receive: Mutex<VecDeque<R>>,
}

unsafe impl<S: Packet, R: Packet> Send for NetworkQueue<S, R> {}
unsafe impl<S: Packet, R: Packet> Sync for NetworkQueue<S, R> {}

impl<S: Packet, R: Packet> Default for NetworkQueue<S, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Packet, R: Packet> NetworkQueue<S, R> {
    pub(crate) fn new() -> Self {
        Self { 
            send: Default::default(),
            receive: Default::default()
        }
    }

    pub(crate) fn add_sendable(&self, packet: S) {
        self.send.lock().unwrap().push_back(packet);
    }

    pub(crate) fn take_sendable(&self) -> VecDeque<S> {
        std::mem::take(&mut *self.send.lock().unwrap())
    }

    pub(crate) fn add_received(&self, packet: R) {
        self.receive.lock().unwrap().push_back(packet);
    }

    pub(crate) fn take_received(&self) -> VecDeque<R> {
        std::mem::take(&mut *self.receive.lock().unwrap())
    }
}