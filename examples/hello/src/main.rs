use std::{net::SocketAddr, ptr::addr_of};

use nanoserde::{SerBin, DeBin};
use pocket::{client::Client, parse_address, protocol::Packet, server::{Server, ServerEvent}, AnyError};

fn main() {
    let mut args = std::env::args();
    args.next();
    let side = args.next().unwrap_or("client".to_string());
    match side.as_str() {
        "client" => run_client(parse_address(
                &args.next().unwrap_or("localhost".to_string()), Some(9000)
                    ).expect("Invalid ip/address")),
        "server" => run_server(parse_address(
                   &args.next().unwrap_or("0.0.0.0".to_string()), Some(9000)
                    ).expect("Invalid ip/address")),
        other => panic!("invalid option `{}` expected [client|server]", other)
    }
}

fn run_client(server_address: SocketAddr) {
    let client = Client::<ClientPacket, ServerPacket>::start(server_address).expect("Unable to start client");
    println!("connected!");
    println!("<- {:?}", ClientPacket::Ping);
    client.send(ClientPacket::Ping);
    while client.running() {
        for packet in client.received() {
            println!("-> {packet:?}");
            match packet {
                ServerPacket::Ping => {
                    println!("<- {:?}", ClientPacket::Pong);
                    client.send(ClientPacket::Pong)
                },
                ServerPacket::Pong => println!("Received pong!"),
                ServerPacket::Message(msg) => println!("Server message: {msg}"),
            }
        }
    }
    println!("Client got disconnected!")
}

fn run_server(server_address: SocketAddr) {
    let server = Server::<ClientPacket, ServerPacket>::start(server_address).expect("Unable to start server");
    println!("Server ready, waiting for clients to connect");
    loop {
        for event in server.events() {
            match event {
                ServerEvent::Connect(addr) => {
                    println!("{addr:?} connected to server");
                    if let Some(client) = server.client(addr) {
                        let message = ServerPacket::Message("Hello, client!".to_string());
                        println!("<- {:?}", message);
                        client.send(message);
                    }
                }
                ServerEvent::Message(addr, packet) => {
                    if let Some(client) = server.client(addr) {
                        match packet {
                            ClientPacket::Ping => {
                                println!("<- {:?}", ServerPacket::Pong);
                                client.send(ServerPacket::Pong)
                            },
                            ClientPacket::Pong => println!("Received pong!"),
                            ClientPacket::Message(msg) => println!("Client message: {msg}"),
                        }
                    }
                },
                ServerEvent::Disconnect(addr) => println!("{addr:?} disconnected"),
            }
        }
    }
}

#[derive(SerBin, DeBin, Debug)]
enum ClientPacket {
    Ping,
    Pong,
    Message(String),
}

impl Packet for ClientPacket {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        nanoserde::SerBin::ser_bin(self, buffer)
    }

    fn deserialize(buffer: &[u8]) -> Result<Self, AnyError> where Self: Sized {
        Ok(nanoserde::DeBin::deserialize_bin(buffer)?)
    }
}

#[derive(SerBin, DeBin, Debug)]
enum ServerPacket {
    Ping,
    Pong,
    Message(String)
}

impl Packet for ServerPacket {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        nanoserde::SerBin::ser_bin(self, buffer)
    }

    fn deserialize(buffer: &[u8]) -> Result<Self, AnyError> where Self: Sized {
        Ok(nanoserde::DeBin::deserialize_bin(buffer)?)
    }
}