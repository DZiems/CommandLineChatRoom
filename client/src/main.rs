use std::io::prelude::*;
use std::io::{stdin, BufReader, BufWriter, Write};
use std::net::TcpStream;
use std::thread;
use std::sync::mpsc;

const DEST:&str = "127.0.0.1:7878";
const QUIT_MSG:&str = "/quit\n";

fn main() {
    let rstream = TcpStream::connect(DEST)
        .expect("*Failed connection. Check destination address is not in use and port socket is bound.");
    /*server.set_nonblocking(true)
        .expect("failed to initialize nonblocking");
     */
    let wstream = rstream.try_clone()
        .expect("*Failed to clone stream.");
    let quit_info = QUIT_MSG.trim();
    println!("Connection established with {}. Enter '{}' anytime to quit.",
             rstream.peer_addr().expect("*Failed to access server's address info."),
             quit_info
    );
    println!("-------------|Welcome to the Chat Room!|-------------");
    println!("**Please wait to join until asked to enter a username...");
    let mut reader = BufReader::new(rstream);
    let mut writer = BufWriter::new(wstream);

    let (sender, receiver) = mpsc::channel::<String>();

    //READER:
    let mut buffer = String::new();
    thread::spawn(move || loop {
        buffer.clear();
        let recvd = reader.read_line(&mut buffer);
        if let Ok(_quit_msg) = receiver.try_recv() {
            println!("**You left the chat.");
            break;
        }
        match recvd {
            Ok(len) => {
                if len == 0 {
                    break;
                }
                print!("{}", buffer)
            }
            Err(_) => {
                let quit_info = QUIT_MSG.trim();
                println!("**Connection lost! Enter '{}' to exit.", quit_info);
                break;
            }
        }
    });
    //WRITER:
    let mut input = String::new();
    loop {
        input.clear();
        let msg_send = stdin().read_line(&mut input);
        match msg_send {
            Ok(_) => {
                if input == String::from(QUIT_MSG) {
                    let quit_msg = input.clone();
                    if let Ok(()) = sender.send(quit_msg) {}
                    else {
                        //the case where the receiver is already down (when server shuts down)
                        println!("**Exiting.");
                    }
                    break;
                }
                else {
                    let res = writer.write(input.as_bytes());
                    match res {
                        Ok(_) => {
                            writer.flush().expect("**Failed to flush the buffer after writing.")
                        }
                        Err(_) => {
                            let quit_info = QUIT_MSG.trim();
                            println!("**Connection lost! Enter '{}' to exit.", quit_info);
                            break;
                        }
                    }
                }
            }
            Err(_) => {
                println!("**Invalid input, try again.");
            }
        }
    }
}