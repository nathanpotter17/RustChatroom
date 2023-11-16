//enable rocket's macro's. We will use many of them
#[macro_use] extern crate rocket;

//init a channel for sending using TOKIO as async runtime, qnd channels are a way to send things between async tasks.
//serde for Serialization,
use rocket::{State, Shutdown};
use rocket::fs::{relative, FileServer};
use rocket::form::Form;
use rocket::response::stream::{EventStream, Event};
use rocket::serde::{Serialize, Deserialize};
use rocket::tokio::sync::broadcast::{channel, Sender, error::RecvError};
use rocket::tokio::select;

//define routes
#[derive(Debug, Clone, FromForm, Serialize, Deserialize)] //derive Debug format, CLone for dupli messages, Form to message struct, and Serialization using Serde. 
#[serde(crate = "rocket::serde")]

struct Message {
    #[field(validate = len(..30))] //limits on length of room name
    pub room: String,
    #[field(validate = len(..20))] //limits on length of user name
    pub username: String,
    pub message: String,
}

//implement endpoints for POST and GET messages.

//POST the form data to the message path. 
#[post("/message", data = "<form>")]
fn post(form: Form<Message>, queue: &State<Sender<Message>>) { //server state is 'Sender'
    // A send fails if no active subscribers. Fine for now.
    let _res = queue.send(form.into_inner());
}

// GET Requests
// Return type is an inifinte stream of server sent events. These events open a continous connection for the server to send data to 
// the client whenever it wants. Like WebSocket, but only 1 direction.

// This function is async, because server sent events are produced asynchronously.
// handler takes two args, server state, and 'end' of type SHUTDOWN.
// call queue.sub() first to create a receiver to listen for events sent down the channel.
// use generator syntax to yield infinite server sent events.
// inside the loop, SELECT waits on multiple concurrent branches, and return as soon as one completes.
// first branch is rx.recv() which waits for new messages, once matched, map the msg. 
// rx.recv returns an enum. if we get type Ok, simply return the msg from the receiver.
// break infinite loop when there are no more senders, or 'Closed' error variant.
// also break infinite loop when the receiver lagged too far behind, or 'Lagged' error variant. Skip to the next iteration via continue.
// second branch is just waiting for 'Shutdown' to resolve, then break.
// if select is processes Ok, yield new server sent event passing the msg.

#[get("/events")]
async fn events(queue: &State<Sender<Message>>, mut end: Shutdown) -> EventStream![] {
    let mut rx = queue.subscribe();
    
    EventStream! {
        loop {
            let msg = select! {
                msg = rx.recv() => match msg {
                    Ok(msg) => msg,
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(_)) => continue,
                },
                _ = &mut end => break,
            };

            yield Event::json(&msg);
        }
    }
}

// start rocket server instance, then mount the route to init the rocket lifecycle.
//add state to instance using manage.
#[launch]
fn rocket() -> _ {
    rocket::build()
        .manage(channel::<Message>(1024).0) //pass in Message struct, capacity of 1024, and get the first element in the tuple because we only want to store the SENDER end in state.
        .mount("/", routes![post, events]) //HTTP namespace, then use routes macro, pass in name of functions to the desired route.
        .mount("/", FileServer::from(relative!("static"))) // handler to serve static files for frontend
}