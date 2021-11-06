use enet::*;
use std::thread;
use std::fmt::Display;
use std::time::Duration;
use std::sync::mpsc::Receiver;

const TICK_RATE: u64 = 1000 / 30;

pub enum NetworkCommand {
    AttemptConnection(Address)
}

pub fn network_main(command_receiver: Receiver<NetworkCommand>) {
    thread::spawn(move || {
        fn handle_error<T, E: Display>(res: Result<T, E>, error_prelude: &str) -> T {
            match res {
                Ok(s) => { s }
                Err(e) => {
                    tfd::message_box_ok("Network init error", &format!("{}: {}", error_prelude, e), tfd::MessageBoxIcon::Error);
                    panic!("Network thread dead");
                }
            }
        }

        let enet = handle_error(Enet::new(), "Unable to initialize Enet");
        let mut host = handle_error(enet.create_host::<u8>(
            None,
            1,
            ChannelLimit::Maximum,
            BandwidthLimit::Unlimited,
            BandwidthLimit::Unlimited
        ), "Unable to create host");

        let mut current_peer = None; //Represents the connection to the server
        loop {
            //Service commands from the main thread
            while let Ok(command) = command_receiver.try_recv() {
                match command {
                    NetworkCommand::AttemptConnection(address) => {
                        current_peer = Some(handle_error(host.connect(&address, 1, 0), "Error attempting connection"));
                    }
                }
            }

            //Process data from the server
            while let Ok(Some(event)) = host.service(0) {
                match &event {
                    Event::Connect(peer) => {
                        println!("Received connect: {:?}", peer);
                    }
                    Event::Disconnect(peer, _) => {
                        println!("Received disconnect: {:?}", peer);

                    }
                    Event::Receive { sender, channel_id, packet } => {
                        println!("Received packet");
                    }
                }
            }

            thread::sleep(Duration::from_millis(TICK_RATE));
        }
    });
}