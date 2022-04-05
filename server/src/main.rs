use server::ThreadPool; //custom-sized collection of threads who receive jobs from a queue
use std::io::prelude::*;
use std::io::{BufReader, BufWriter, Write};
use std::net::{TcpStream, TcpListener};
use std::sync::{Arc, Mutex, MutexGuard}; //for sharing access to the list of all clients from each client thread
use std::sync::mpsc::{self, Sender};
use std::collections::HashMap;

const MSG_SIZE: usize = 128;
const NAME_SIZE: usize = 16;
const LOCAL: &str = "127.0.0.1:7878";
const MAX_CLIENTS: usize = 4;
//juvenile error propagation helper (not great with the Result<> propagation yet)
const ERROR_MSG: &str = "#%#---#%#*Err*#%#---#%#";


//simple function to write to the client and flush so no processes occur until writing has fully
//completed.
fn write_bufwriter(writer: &mut BufWriter<TcpStream>, message: &str) {
    writer.write(message.as_bytes()).expect("*Failed to write msg.");
    writer.flush().expect("*Failed to flush the buffer after writing.");
}

//part of initializing a client into the chat room
//need to pass in list of clients to check if the entered username has already been designated
//need the writer to tell the user to re-enter in this case
fn request_username(reader: &mut BufReader<TcpStream>,
                    writer: &mut BufWriter<TcpStream>,
                    clients: &mut MutexGuard<HashMap<String, BufWriter<TcpStream>>>) -> String {
    let mut buffer = String::with_capacity(NAME_SIZE);
    loop {
        buffer.clear();
        match reader.read_line(&mut buffer) {
            Ok(_) => {
                //trim the newline, then resize the name entry into a NAME_SIZE sized String
                let mut username = String::from(buffer.trim());
                username.truncate(NAME_SIZE);
                if clients.contains_key(&username) {
                    print!("*Username taken, please try another.\n");
                    write_bufwriter(writer, "*Username taken, please try another.\n")
                }
                else {
                    if username.len() > 0 {
                        return username;
                    } else {
                        let default = String::from("Default_Username");
                        return default;
                    }
                }

            }
            Err(_) => return String::from(ERROR_MSG)
        }
    }
}

//wait for a message from a user
//return the entered message, first appending the user's name
//example: "Username: Hello there.\n"
fn receive_msg(reader: &mut BufReader<TcpStream>, username: &String) -> String {
    let mut buffer = String::with_capacity(MSG_SIZE);
    match reader.read_line(&mut buffer) {
        Ok(_) => {
            let mut msg = format!("{}: {}", username, &*buffer);
            //when the client does a read_line(), there needs to be a \n at the end tell the reader
            //to finish scanning the buffer
            if msg.len() > MSG_SIZE {
                msg.truncate(MSG_SIZE - 1);
                msg.push_str("\n");
                msg.truncate(MSG_SIZE);
            }
            msg
        }
        Err(_) => String::from(ERROR_MSG)
    }

}

//pass in the list of clients (a mutex protected vector of TcpStream BufWriters)
//send message to each client, including the user who sent it
fn broadcast(clients: &mut MutexGuard<HashMap<String, BufWriter<TcpStream>>>,
             message: &String) {
    let msg = message.as_str();
    for (_, client) in  clients.iter_mut() {
        write_bufwriter( &mut *client,
                         msg);
    }
}

//in a loop, receive messages and send them to the broadcasting receiver
//if at any time the error message has come through, break out of loop
fn handle_connection(reader: &mut BufReader<TcpStream>,
                     //writer: &mut BufWriter<TcpStream>,
                     username: &String,
                     sender: Sender<String>)
{
    loop {
        let msg = receive_msg(&mut *reader, &username);
        if msg == String::from(ERROR_MSG) {
            let exit_msg = format!("*{} has left.\n", &username);
            if let Ok(()) = sender.send(exit_msg.clone()) {}
            else {
                println!("DEBUG: exit message from {} could not be relayed to the broadcasting receiver.", &username);
            }
            break;
        }
        if let Ok(()) = sender.send(msg.clone()) {}
        else {
            println!("DEBUG: message from {} could not be relayed to the broadcasting receiver.", &username);
        }
    }
}

fn main() {
    println!("-------------|Welcome to the Chat Room! Size = {}|-------------", MAX_CLIENTS);
    let server:TcpListener = TcpListener::bind(LOCAL)
        .expect("*Listener failed to bind.");


    //declare threadpool--number of threads is number of clients + 1 for server receiver thread
    let pool = ThreadPool::new(MAX_CLIENTS + 1);
    //clients are stored in a thread-safe hashmap, mapping a user's entered username
    //with their corresponding TCP connection (Meaning a user's name must be unique).
    let clients: Arc<Mutex<HashMap<String, BufWriter<TcpStream>>>> =
        Arc::new(Mutex::new(HashMap::with_capacity(MAX_CLIENTS)));


    /*
    Use mpsc to create a sender and receiver, which are essentially a polling system; sender will send
    a signal to the receiver after something happens.

    In this case, we want the sender to tell the receiver when a message has been received by a client.
    When that happens, the receiver broadcasts the message to all clients in the room.
     */
    let (sender, receiver) = mpsc::channel::<String>();
    let clients_bc = Arc::clone(&clients);
    pool.execute(move|| {
        loop {
            if let Ok(msg) = receiver.try_recv() {
                print!("{}", msg);
                broadcast(&mut clients_bc.lock().unwrap(),
                          &msg);
            }
        }
    });

    //process incoming connections
    for rstream in server.incoming() {
        match rstream {
            Err(_) => println!("*Failed to generate TcpStream for requested connection."),
            Ok(rstream) => {
                println!("**Incoming connection from {}.", rstream.peer_addr().unwrap());
                //get copies of the stream for a writer and broadcast list
                let wstream = rstream.try_clone()
                    .expect("*Failed to clone stream");
                let clients_add = rstream.try_clone().unwrap();

                //initialize reader and writer for individual client-serve message communication:
                let mut reader = BufReader::new(rstream);
                let mut writer = BufWriter::new(wstream);
                //initialize writer for the broadcast message communication:
                let clients_add = BufWriter::new(clients_add);

                //check if there is space for this new client in the room
                let clients_mut = Arc::clone(&clients);
                let len = clients_mut.lock()
                    .expect("*Failed to mutex lock clients list").len();
                if len < MAX_CLIENTS {
                    println!("**Initializing client ({}) and requesting a username...", len);

                    //spawn a thread for the client, sender_c will relay any messages to the
                    //broadcasting receiver, clients_ir will allow access to clients list
                    let sender_c = sender.clone();
                    //let clients_mut = Arc::clone(&clients);
                    pool.execute(move|| {
                        //Initialize username:
                        write_bufwriter(&mut writer, "*Please enter a username: \n");
                        let username = request_username(&mut reader, &mut writer, &mut clients_mut.lock().expect("Failed to mutex lock clients list"));
                        if username != String::from(ERROR_MSG)
                        {
                            clients_mut.lock()
                                .expect("Failed to mutex lock clients list").insert(username.clone(), clients_add);
                            println!("**Client '{}' has joined.", username);
                            //broadcast arrival
                            let new_arrival_msg = format!("*{} has joined. Say hello!\n", username.clone());
                            if let Ok(()) = sender_c.send(new_arrival_msg) {} else {
                                println!("DEBUG: message from {} could not be relayed to the broadcasting receiver.", &username);
                            }
                            //receive messages and write them to broadcasting receiver in a loop
                            handle_connection(&mut reader, /*&mut writer, */&username, sender_c);
                            //at the end of the connection, remove the client
                            clients_mut.lock()
                                .expect("*Failed to mutex lock clients list").remove(&username);
                            println!("**Client '{}' has been successfully removed.", username);
                        }
                        else {
                            println!("**A client left before entering a username.");
                        }
                    });
                }
                else {
                    println!("Refusing a request to join; room is full.");
                    let refuse_msg = format!("Room is already at {0}/{0} capacity. Please quit and try again later.\n", MAX_CLIENTS);
                    write_bufwriter(&mut writer, refuse_msg.as_str());
                }
            }
        }
    }

}
