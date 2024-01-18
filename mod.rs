mod socket_message;

use futures::{
	future::{select, Either},
	stream::StreamExt,
	FutureExt, SinkExt,
};
use hyper::{server::conn::http1, service::service_fn, upgrade::Upgraded};
use hyper_tungstenite::upgrade;
use hyper_util::rt::TokioIo;
use serde_json::Value;
use socket_message::{SocketMessage, SocketMessageBuilder};
use std::{borrow::Cow, collections::HashMap, convert::Infallible, net::SocketAddr, time::Duration};
use tokio::{
	net::TcpListener,
	sync::mpsc::{channel, Receiver},
	time::sleep,
};
use tokio_tungstenite::WebSocketStream;
use tungstenite::{
	protocol::{frame::coding::CloseCode, CloseFrame},
	Message,
};
use uuid::Uuid;

#[derive(Debug)]
pub struct ConnectionDetails {
	pub path: String,
	pub query_params: HashMap<String, String>,
}

#[derive(Debug)]
pub enum Event {
	Connect(ConnectionDetails),
	Update(Value),
}

pub struct Connection {
	connection_details: Option<ConnectionDetails>,
	socket: WebSocketStream<TokioIo<Upgraded>>,
	last_value: Option<Value>,
	client_pin: Option<Uuid>,
}

impl Connection {
	pub async fn next_event_with_timeout(&mut self, timeout: Duration) -> Option<Event> {
		let res = {
			let foo = select(self.next_event().boxed(), sleep(timeout).boxed()).await;

			match foo {
				Either::Left((event, _)) => Some(event),
				Either::Right(_) => None,
			}
		};

		match res {
			Some(event) => event,
			None => {
				self.close("Connection closed due to inactivity. When another operation is necessary, reconnect")
					.await;

				None
			}
		}
	}

	pub async fn next_event(&mut self) -> Option<Event> {
		None
	}

	pub async fn next_socket_event(&mut self) -> Option<Event> {
		match self.connection_details.take() {
			Some(details) => return Some(Event::Connect(details)),
			None => (),
		}

		let model = loop {
			let message = match self.socket.next().await {
				Some(message) => match message {
					Ok(message) => message,
					Err(_) => {
						self.close("Error parsing incoming message").await;
						return None;
					}
				},
				None => return None,
			};

			let socket_message = match message {
				Message::Text(text) => match SocketMessage::parse(text) {
					Ok(message) => message,
					Err(_) => {
						self.close("Failed to parse socket message").await;
						return None;
					}
				},
				Message::Ping(_) | Message::Pong(_) => continue,
				Message::Close(_) => return None,
				_ => {
					self.close("Received invalid message").await;
					return None;
				}
			};

			let prefix = socket_message.get_prefix();

			if prefix == "sync" {
				match self.last_value.take() {
					Some(model) => self.send(model).await,
					None => (),
				};

				continue;
			} else if prefix == "event" {
				let pin = match socket_message.get_context() {
					Some(context) => match Uuid::parse_str(context) {
						Ok(uuid) => Some(uuid),
						Err(_) => {
							self.close("Expected to receive a valid UUID a context to 'event' message").await;

							return None;
						}
					},
					None => None,
				};

				self.client_pin = pin;

				match socket_message.get_body() {
					Some(body) => break body,
					None => {
						self.close("Expected to receive JSON body with 'event' message").await;

						return None;
					}
				}
			} else {
				self.close("Invalid message prefix: ".to_owned() + prefix + ". Expected 'sync' or 'event'")
					.await;

				return None;
			}
		};

		Some(Event::Update(model))
	}

	pub async fn send(&mut self, state: Value) {
		let pin_string = match self.client_pin {
			Some(pin) => pin.to_string(),
			None => "".to_owned(),
		};

		// This should never panic because we are definitely setting it up correctly here
		let message = SocketMessageBuilder::new("model").context(pin_string).body(&state).build().unwrap();

		let _ = self.socket.send(Message::Text(message.to_string())).await;

		self.last_value.replace(state);
	}

	pub async fn close<S: Into<String>>(&mut self, reason: S) {
		let _ = self
			.socket
			.close(Some(CloseFrame {
				code: CloseCode::Normal,
				reason: Cow::Owned(reason.into()),
			}))
			.await;
	}
}

pub struct SocketServer {
	connections_receiver: Receiver<Connection>,
}

impl SocketServer {
	pub async fn new(port: u16) -> SocketServer {
		let addr = SocketAddr::from(([127, 0, 0, 1], port));
		let listener = TcpListener::bind(addr).await.unwrap();

		let mut http = http1::Builder::new();
		http.keep_alive(true);

		let (sender, receiver) = channel(100);

		tokio::spawn(async move {
			loop {
				let (stream, _) = listener.accept().await.unwrap();
				let connection_sender = sender.clone();

				let connection = http
					.serve_connection(
						TokioIo::new(stream),
						service_fn(move |mut request| {
							let single_sender = connection_sender.clone();

							async move {
								let uri = request.uri();
								let path = uri.path().to_owned();
								let mut query_params = HashMap::new();

								for (key, value) in form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()) {
									query_params.insert(key.to_string(), value.to_string());
								}

								let (response, hyper_socket) = upgrade(&mut request, None).unwrap();

								tokio::spawn(async move {
									let socket = hyper_socket.await.unwrap();

									match single_sender
										.clone()
										.send(Connection {
											connection_details: Some(ConnectionDetails { path, query_params }),
											socket,
											client_pin: None,
											last_value: None,
										})
										.await
									{
										Ok(_) => (),
										Err(mut error) => {
											error.0.close("Failed to queue connection").await;
										}
									};
								});

								Ok::<_, Infallible>(response)
							}
						}),
					)
					.with_upgrades();

				tokio::spawn(async move {
					connection.await.unwrap();
				});
			}
		});

		SocketServer {
			connections_receiver: receiver,
		}
	}

	pub async fn accept_connection(&mut self) -> Option<Connection> {
		self.connections_receiver.recv().await
	}
}
