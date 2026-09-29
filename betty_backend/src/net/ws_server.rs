use std::net::SocketAddr;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message};

use crate::bridge::{FromFront, ToFront};

pub struct WsBridgeServer;

impl WsBridgeServer {
    pub async fn start(
        addr: &str,
        tx_to_diplomat: mpsc::UnboundedSender<FromFront>,
        tx_to_front: broadcast::Sender<ToFront>,
    ) {
        let socket_addr: SocketAddr = addr.parse().expect("Некорректный IP:порт для WebSocket сервера");
        let listener = TcpListener::bind(socket_addr).await.expect("Не удалось занять порт WebSocket");
        println!("🌐 [WebSocket]: Сервер слушает на ws://{}", addr);

        while let Ok((stream, peer)) = listener.accept().await {
            println!("🤝 [WebSocket]: Новое подключение от: {}", peer);
            let tx_dip = tx_to_diplomat.clone();
            let rx_front = tx_to_front.subscribe();
            tokio::spawn(async move {
                Self::handle_connection(stream, peer, tx_dip, rx_front).await;
            });
        }
    }

    async fn handle_connection(
        stream: TcpStream,
        peer: SocketAddr,
        tx_to_diplomat: mpsc::UnboundedSender<FromFront>,
        mut rx_to_front: broadcast::Receiver<ToFront>,
    ) {
        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("[WebSocket]: Ошибка рукопожатия с {}: {:?}", peer, e);
                return;
            }
        };

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        loop {
            tokio::select! {
                event_result = rx_to_front.recv() => {
                    match event_result {
                        Ok(event) => {
                            if let Ok(json_str) = serde_json::to_string(&event) {
                                if let Err(e) = ws_sender.send(Message::Text(json_str)).await {
                                    eprintln!("[WebSocket]: Ошибка отправки на {}: {:?}", peer, e);
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(missed)) => {
                            eprintln!("[WebSocket]: Клиент {} пропустил {} сообщений (отставание буфера)", peer, missed);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<FromFront>(&text) {
                                Ok(cmd) => {
                                    if let Err(e) = tx_to_diplomat.send(cmd) {
                                        eprintln!("[WebSocket]: Ошибка отправки команды в Diplomat: {:?}", e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[WebSocket]: Не удалось распарсить JSON команду: {:?}, payload: {}", e, text);
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            println!("[WebSocket]: Клиент {} отключился.", peer);
                            break;
                        }
                        Some(Ok(Message::Ping(p))) => {
                            let _ = ws_sender.send(Message::Pong(p)).await;
                        }
                        Some(Err(e)) => {
                            eprintln!("[WebSocket]: Ошибка сокета {}: {:?}", peer, e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
