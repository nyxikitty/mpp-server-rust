use crate::handlers::MessageHandler;
use crate::types::{BanInfo, Channel, ChannelSettings, ClientData, Crown, NoteQuota, Position};
use crate::utils::{current_time_ms, generate_client_id, generate_random_id};
use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

pub struct Server {
    pub channels: DashMap<String, Arc<RwLock<Channel>>>,
    pub clients: DashMap<String, Arc<RwLock<ClientData>>>,
    pub subscribed_to_ls: DashMap<String, bool>,
    pub banned_users: DashMap<String, BanInfo>,
    pub ws_senders: DashMap<String, mpsc::UnboundedSender<String>>,
}

impl Server {
    pub fn new() -> Self {
        let server = Self {
            channels: DashMap::new(),
            clients: DashMap::new(),
            subscribed_to_ls: DashMap::new(),
            banned_users: DashMap::new(),
            ws_senders: DashMap::new(),
        };

        // Start note quota tick loop
        let clients = server.clients.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                for client_ref in clients.iter() {
                    let mut client = client_ref.value().write().await;
                    client.note_quota.tick();
                }
            }
        });

        server
    }

    pub async fn handle_connection(
        self: Arc<Self>,
        socket: WebSocket,
        ip: String,
    ) -> anyhow::Result<()> {
        let client_id = generate_client_id(&ip);
        let connection_id = generate_random_id();

        info!("New connection: client_id={}, connection_id={}", client_id, connection_id);

        // Create or get client
        if !self.clients.contains_key(&client_id) {
            let client_data = ClientData {
                user_id: client_id.clone(),
                participant: None,
                channel_id: None,
                last_move_time: None,
                note_quota: NoteQuota::new(),
            };
            self.clients.insert(client_id.clone(), Arc::new(RwLock::new(client_data)));
        }

        let (mut ws_sender, mut ws_receiver) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        // Store the sender channel
        self.ws_senders.insert(client_id.clone(), tx);
        debug!("Stored WebSocket sender for client: {}", client_id);

        let client_id_for_sender = client_id.clone();

        // Spawn task to handle outgoing messages
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                debug!("Outgoing to {}: {}", client_id_for_sender, msg);
                if let Err(e) = ws_sender.send(Message::Text(msg)).await {
                    error!("Failed to send WebSocket message: {}", e);
                    break;
                }
            }
            debug!("Sender task ended for {}", client_id_for_sender);
        });

        let message_handler = MessageHandler::new(self.clone());
        let client_id_clone = client_id.clone();
        let self_clone = self.clone();

        // Handle incoming messages
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received message from {}: {}", client_id, text);
                    
                    match serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                        Ok(messages) => {
                            for msg_value in messages {
                                if let Ok(msg) = serde_json::from_value(msg_value) {
                                    if let Some(response) = message_handler
                                        .handle_message(&client_id, msg)
                                        .await
                                    {
                                        let response_str = serde_json::to_string(&response)?;
                                        self_clone.send_to_client(&client_id, &response_str).await;
                                    }
                                } else {
                                    error!("Failed to parse message");
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse messages array: {}", e);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Client {} closed connection", client_id);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error for client {}: {}", client_id, e);
                    break;
                }
                _ => {}
            }
        }

        // Cleanup on disconnect
        self_clone.handle_disconnect(&client_id_clone).await;
        self_clone.ws_senders.remove(&client_id_clone);

        Ok(())
    }

    pub async fn handle_disconnect(&self, client_id: &str) {
        info!("Handling disconnect for client: {}", client_id);

        if let Some(client_ref) = self.clients.get(client_id) {
            let client = client_ref.value().read().await;

            if let Some(channel_id) = &client.channel_id {
                if let Some(channel_ref) = self.channels.get(channel_id) {
                    let mut channel = channel_ref.value().write().await;
                    channel.participants.remove(client_id);

                    // Handle crown transfer
                    if let Some(crown) = &mut channel.crown {
                        if crown.participant_id.as_deref() == Some(client_id) {
                            crown.participant_id = None;
                            crown.user_id = None;
                        }
                    }
                    
                    // Transfer crown after releasing the mutable borrow
                    let needs_crown_transfer = channel.crown.as_ref()
                        .map(|c| c.participant_id.is_none())
                        .unwrap_or(false);
                    
                    if needs_crown_transfer {
                        let first_id = channel.participants.keys().next().cloned();
                        if let Some(first_id) = first_id {
                            if let Some(crown) = &mut channel.crown {
                                crown.participant_id = Some(first_id.clone());
                                if let Some(client_ref) = self.clients.get(&first_id) {
                                    let client = client_ref.value().read().await;
                                    crown.user_id = Some(client.user_id.clone());
                                }
                                crown.time = current_time_ms();
                            }
                        }
                    }

                    // Broadcast bye message
                    let bye_msg = serde_json::json!([{
                        "m": "bye",
                        "p": client_id
                    }]);
                    self.broadcast_to_channel(channel_id, &bye_msg, Some(client_id))
                        .await;

                    // Delete empty non-special channels
                    if channel.participants.is_empty()
                        && channel._id != "lobby"
                        && !channel._id.starts_with("test/")
                    {
                        drop(channel);
                        self.channels.remove(channel_id);
                        self.broadcast_ls_update(channel_id, false).await;
                    }
                }
            }
        }

        self.subscribed_to_ls.remove(client_id);
        self.clients.remove(client_id);
    }

    pub async fn broadcast_to_channel(
        &self,
        channel_id: &str,
        messages: &serde_json::Value,
        exclude_client_id: Option<&str>,
    ) {
        if let Some(channel_ref) = self.channels.get(channel_id) {
            let channel = channel_ref.value().read().await;

            let msg_str = match serde_json::to_string(messages) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                    return;
                }
            };

            debug!("Broadcasting to channel {}: {} participants", channel_id, channel.participants.len());

            for (participant_id, _) in channel.participants.iter() {
                if Some(participant_id.as_str()) != exclude_client_id {
                    debug!("Sending to participant: {}", participant_id);
                    self.send_to_client(participant_id, &msg_str).await;
                }
            }
        } else {
            debug!("Tried to broadcast to non-existent channel: {}", channel_id);
        }
    }

    pub async fn send_to_client(&self, client_id: &str, message: &str) {
        if let Some(sender) = self.ws_senders.get(client_id) {
            debug!("Sending to {}: {}", client_id, message);
            if let Err(e) = sender.send(message.to_string()) {
                error!("Failed to send message to client {}: {}", client_id, e);
            }
        } else {
            debug!("No WebSocket sender found for client: {}", client_id);
        }
    }

    pub async fn broadcast_ls_update(&self, channel_id: &str, is_bulk: bool) {
        if let Some(channel_ref) = self.channels.get(channel_id) {
            let channel = channel_ref.value().read().await;

            if !channel.settings.visible {
                return;
            }

            let message = serde_json::json!([{
                "m": "ls",
                "c": is_bulk,
                "u": [{
                    "_id": channel._id,
                    "count": channel.participants.len(),
                    "crown": if channel.settings.lobby { None } else { channel.crown.as_ref() },
                    "settings": &channel.settings
                }]
            }]);

            let msg_str = match serde_json::to_string(&message) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to serialize ls update: {}", e);
                    return;
                }
            };

            for subscriber in self.subscribed_to_ls.iter() {
                self.send_to_client(subscriber.key(), &msg_str).await;
            }
        }
    }

    pub fn create_default_channel(&self, channel_id: &str) -> Channel {
        let is_special = channel_id == "lobby" || channel_id.starts_with("test/");

        let settings = if is_special {
            ChannelSettings {
                color: "#73b3cc".to_string(),
                color2: Some("#273546".to_string()),
                lobby: true,
                visible: true,
                chat: Some(true),
                crownsolo: None,
            }
        } else {
            ChannelSettings {
                color: "#ecfaed".to_string(),
                color2: None,
                lobby: false,
                visible: true,
                chat: None,
                crownsolo: None,
            }
        };

        let crown = if is_special {
            None
        } else {
            Some(Crown {
                participant_id: None,
                user_id: None,
                time: current_time_ms(),
                start_pos: Position { x: 0.0, y: 0.0 },
                end_pos: Position { x: 0.0, y: 0.0 },
            })
        };

        Channel {
            _id: channel_id.to_string(),
            settings,
            crown,
            participants: Default::default(),
            chat_history: Vec::new(),
        }
    }
}