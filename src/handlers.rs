use crate::server::Server;
use crate::types::{Crown, IncomingMessage, Participant, Position};
use crate::utils::current_time_ms;
use std::sync::Arc;
use tracing::{debug, warn};

pub struct MessageHandler {
    server: Arc<Server>,
}

impl MessageHandler {
    pub fn new(server: Arc<Server>) -> Self {
        Self { server }
    }

    pub async fn handle_message(
        &self,
        client_id: &str,
        msg: IncomingMessage,
    ) -> Option<Vec<serde_json::Value>> {
        debug!("Handling message type '{}' from {}", msg.m, client_id);

        match msg.m.as_str() {
            "hi" => self.handle_hi(client_id).await,
            "bye" => {
                self.handle_bye(client_id).await;
                None
            }
            "+ls" => self.handle_plus_ls(client_id).await,
            "-ls" => {
                self.handle_minus_ls(client_id).await;
                None
            }
            "t" => self.handle_time(&msg.data).await,
            "a" => {
                self.handle_chat(client_id, &msg.data).await;
                None
            }
            "n" => {
                self.handle_note(client_id, &msg.data).await;
                None
            }
            "m" => {
                self.handle_movement(client_id, &msg.data).await;
                None
            }
            "userset" => {
                self.handle_userset(client_id, &msg.data).await;
                None
            }
            "ch" => {
                self.handle_channel(client_id, &msg.data).await;
                None
            }
            "chset" => {
                self.handle_channel_settings(client_id, &msg.data).await;
                None
            }
            "chown" => {
                self.handle_chown(client_id, &msg.data).await;
                None
            }
            "kickban" => {
                self.handle_kickban(client_id, &msg.data).await;
                None
            }
            "unban" => {
                self.handle_unban(client_id, &msg.data).await;
                None
            }
            "devices" => self.handle_devices(client_id, &msg.data).await,
            _ => {
                warn!("Unknown message type '{}' from {}", msg.m, client_id);
                None
            }
        }
    }

    async fn handle_hi(&self, client_id: &str) -> Option<Vec<serde_json::Value>> {
        let client_ref = self.server.clients.get(client_id)?;
        let mut client = client_ref.value().write().await;

        let participant = Participant {
            id: client_id.to_string(),
            _id: client.user_id.clone(),
            name: "Anonymous".to_string(),
            color: format!("#{}", &client.user_id[..6.min(client.user_id.len())]),
            x: 0.0,
            y: 0.0,
        };

        client.participant = Some(participant.clone());

        let response = vec![
            serde_json::json!({
                "m": "hi",
                "u": participant,
                "t": current_time_ms(),
                "v": "1.0.0",
                "motd": "Welcome to Multiplayer Piano!"
            }),
            client.note_quota.get_params(),
        ];

        Some(response)
    }

    async fn handle_bye(&self, client_id: &str) {
        self.server.handle_disconnect(client_id).await;
    }

    async fn handle_plus_ls(&self, client_id: &str) -> Option<Vec<serde_json::Value>> {
        self.server.subscribed_to_ls.insert(client_id.to_string(), true);

        let mut channels_data = Vec::new();
        for channel_ref in self.server.channels.iter() {
            let channel = channel_ref.value().read().await;
            if channel.settings.visible {
                channels_data.push(serde_json::json!({
                    "_id": channel._id,
                    "count": channel.participants.len(),
                    "crown": if channel.settings.lobby { None } else { channel.crown.as_ref() },
                    "settings": &channel.settings
                }));
            }
        }

        Some(vec![serde_json::json!({
            "m": "ls",
            "c": true,
            "u": channels_data
        })])
    }

    async fn handle_minus_ls(&self, client_id: &str) {
        self.server.subscribed_to_ls.remove(client_id);
    }

    async fn handle_time(&self, data: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
        let e = data.get("e")?;

        Some(vec![serde_json::json!({
            "m": "t",
            "t": current_time_ms(),
            "e": e
        })])
    }

    async fn handle_chat(&self, client_id: &str, data: &serde_json::Value) {
        let message = match data.get("message").and_then(|m| m.as_str()) {
            Some(m) => m,
            None => return,
        };
        
        if message.len() > 256 {
            return;
        }

        let client_ref = match self.server.clients.get(client_id) {
            Some(c) => c,
            None => return,
        };
        
        let client = client_ref.value().read().await;
        let channel_id = match client.channel_id.as_ref() {
            Some(id) => id.clone(),
            None => return,
        };
        
        let participant = match client.participant.as_ref() {
            Some(p) => p.clone(),
            None => return,
        };
        drop(client);

        let channel_ref = match self.server.channels.get(&channel_id) {
            Some(c) => c,
            None => return,
        };
        
        let channel = channel_ref.value().write().await;

        if !channel.settings.chat.unwrap_or(false) {
            return;
        }

        let chat_msg = serde_json::json!({
            "m": "a",
            "a": &message[..256.min(message.len())],
            "p": participant,
            "t": current_time_ms()
        });

        drop(channel);
        self.server.broadcast_to_channel(&channel_id, &serde_json::json!([chat_msg]), None).await;
    }

    async fn handle_note(&self, client_id: &str, data: &serde_json::Value) {
        let notes = match data.get("n").and_then(|n| n.as_array()) {
            Some(n) => n,
            None => return,
        };
        
        let needed = notes.len() as i32;

        let client_ref = match self.server.clients.get(client_id) {
            Some(c) => c,
            None => return,
        };
        
        let mut client = client_ref.value().write().await;

        if !client.note_quota.spend(needed) {
            warn!("Client {} exceeded note quota", client_id);
            let notification = serde_json::json!([{
                "m": "notification",
                "text": "You're playing too fast! Slow down.",
                "class": "short",
                "duration": 2000
            }]);
            let msg_str = serde_json::to_string(&notification).unwrap_or_default();
            drop(client);
            self.server.send_to_client(client_id, &msg_str).await;
            return;
        }

        let channel_id = match client.channel_id.as_ref() {
            Some(id) => id.clone(),
            None => return,
        };
        drop(client);

        let channel_ref = match self.server.channels.get(&channel_id) {
            Some(c) => c,
            None => return,
        };
        
        let channel = channel_ref.value().read().await;

        if let Some(crownsolo) = channel.settings.crownsolo {
            if crownsolo {
                if let Some(crown) = &channel.crown {
                    if crown.participant_id.as_deref() != Some(client_id) {
                        return;
                    }
                }
            }
        }

        let note_msg = serde_json::json!({
            "m": "n",
            "t": data.get("t"),
            "n": notes,
            "p": client_id
        });

        drop(channel);
        self.server.broadcast_to_channel(&channel_id, &serde_json::json!([note_msg]), Some(client_id)).await;
    }

    async fn handle_movement(&self, client_id: &str, data: &serde_json::Value) {
        let x = match data.get("x") {
            Some(v) => {
                if let Some(f) = v.as_f64() {
                    f
                } else if let Some(s) = v.as_str() {
                    match s.parse::<f64>() {
                        Ok(f) => f,
                        Err(_) => return,
                    }
                } else {
                    return;
                }
            }
            None => return,
        };
        
        let y = match data.get("y") {
            Some(v) => {
                if let Some(f) = v.as_f64() {
                    f
                } else if let Some(s) = v.as_str() {
                    match s.parse::<f64>() {
                        Ok(f) => f,
                        Err(_) => return,
                    }
                } else {
                    return;
                }
            }
            None => return,
        };

        let client_ref = match self.server.clients.get(client_id) {
            Some(c) => c,
            None => return,
        };
        
        let mut client = client_ref.value().write().await;

        let now = current_time_ms();
        if let Some(last_move) = client.last_move_time {
            if now - last_move < 50 {
                return;
            }
        }
        client.last_move_time = Some(now);

        if let Some(participant) = &mut client.participant {
            participant.x = x;
            participant.y = y;
        }

        let channel_id = match client.channel_id.as_ref() {
            Some(id) => id.clone(),
            None => return,
        };
        drop(client);

        tracing::debug!("Broadcasting movement for {} to channel {}", client_id, channel_id);

        let movement = serde_json::json!({
            "m": "m",
            "id": client_id,
            "x": x,
            "y": y
        });

        self.server.broadcast_to_channel(&channel_id, &serde_json::json!([movement]), Some(client_id)).await;
    }

    async fn handle_userset(&self, client_id: &str, data: &serde_json::Value) {
        let set = match data.get("set") {
            Some(s) => s,
            None => return,
        };
        
        let name = match set.get("name").and_then(|n| n.as_str()) {
            Some(n) => n,
            None => return,
        };
        
        let trimmed_name = name.trim();

        if trimmed_name.is_empty() || trimmed_name.len() > 40 {
            return;
        }

        let client_ref = match self.server.clients.get(client_id) {
            Some(c) => c,
            None => return,
        };
        
        let mut client = client_ref.value().write().await;

        if let Some(participant) = &mut client.participant {
            participant.name = trimmed_name.to_string();
            if let Some(color) = set.get("color").and_then(|c| c.as_str()) {
                participant.color = color.to_string();
            }
        }

        let channel_id = match client.channel_id.as_ref() {
            Some(id) => id.clone(),
            None => return,
        };
        
        let user_id = client.user_id.clone();
        let participant = match client.participant.as_ref() {
            Some(p) => p.clone(),
            None => return,
        };
        drop(client);

        let update = serde_json::json!({
            "m": "p",
            "id": client_id,
            "_id": user_id,
            "name": participant.name,
            "color": participant.color,
            "x": participant.x,
            "y": participant.y
        });

        self.server.broadcast_to_channel(&channel_id, &serde_json::json!([update]), None).await;
    }

    async fn handle_channel(&self, client_id: &str, data: &serde_json::Value) {
        let channel_id = match data.get("_id").and_then(|id| id.as_str()) {
            Some(id) => id,
            None => return,
        };
        
        let channel_id = if channel_id.len() > 512 { "lobby" } else { channel_id };

        let client_ref = match self.server.clients.get(client_id) {
            Some(c) => c,
            None => return,
        };
        
        let client = client_ref.value().read().await;
        let user_id = client.user_id.clone();
        drop(client);

        if let Some(ban) = self.server.banned_users.get(&user_id) {
            if ban.channel_id == channel_id && ban.expiry > current_time_ms() {
                let notification = serde_json::json!([{
                    "m": "notification",
                    "id": format!("Notification-ban-{}", current_time_ms()),
                    "title": "",
                    "text": format!("You are banned from {} until {}.", 
                        channel_id, 
                        chrono::DateTime::<chrono::Utc>::from_timestamp((ban.expiry / 1000) as i64, 0)
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_default()
                    ),
                    "class": "short",
                    "duration": 5000
                }]);
                let msg_str = serde_json::to_string(&notification).unwrap_or_default();
                self.server.send_to_client(client_id, &msg_str).await;
                return;
            }
        }

        if !self.server.channels.contains_key(channel_id) {
            let channel = self.server.create_default_channel(channel_id);
            self.server.channels.insert(channel_id.to_string(), Arc::new(tokio::sync::RwLock::new(channel)));
            self.server.broadcast_ls_update(channel_id, false).await;
        }

        let mut client = client_ref.value().write().await;
        if let Some(old_channel_id) = &client.channel_id {
            if old_channel_id != channel_id {
                if let Some(channel_ref) = self.server.channels.get(old_channel_id) {
                    let mut channel = channel_ref.value().write().await;
                    channel.participants.remove(client_id);

                    if let Some(crown) = &mut channel.crown {
                        if crown.participant_id.as_deref() == Some(client_id) {
                            crown.participant_id = None;
                            crown.user_id = None;
                        }
                    }
                }
                
                let bye_msg = serde_json::json!([{
                    "m": "bye",
                    "p": client_id
                }]);
                self.server.broadcast_to_channel(old_channel_id, &bye_msg, Some(client_id)).await;
            }
        }

        client.channel_id = Some(channel_id.to_string());
        
        if client.participant.is_none() {
            client.participant = Some(Participant {
                id: client_id.to_string(),
                _id: user_id.clone(),
                name: "Anonymous".to_string(),
                color: format!("#{}", &user_id[..6.min(user_id.len())]),
                x: 0.0,
                y: 0.0,
            });
        }

        let participant = match client.participant.clone() {
            Some(p) => p,
            None => return,
        };
        drop(client);

        let channel_ref = match self.server.channels.get(channel_id) {
            Some(c) => c,
            None => return,
        };
        
        let mut channel = channel_ref.value().write().await;
        channel.participants.insert(client_id.to_string(), participant.clone());

        if let Some(crown) = &mut channel.crown {
            if crown.participant_id.is_none() {
                crown.participant_id = Some(client_id.to_string());
                crown.user_id = Some(user_id);
                crown.time = current_time_ms();
            }
        }

        let ppl: Vec<_> = channel.participants.values().cloned().collect();
        let chat_history = channel.chat_history.clone();
        let channel_info = serde_json::json!({
            "_id": channel._id,
            "settings": channel.settings,
            "crown": channel.crown
        });

        drop(channel);

        let join_msg = serde_json::json!([
            {
                "m": "ch",
                "ch": channel_info,
                "ppl": ppl,
                "p": client_id
            },
            {
                "m": "c",
                "c": chat_history
            }
        ]);
        let msg_str = serde_json::to_string(&join_msg).unwrap_or_default();
        self.server.send_to_client(client_id, &msg_str).await;

        let participant_msg = serde_json::json!([{
            "m": "p",
            "id": client_id,
            "_id": participant._id,
            "name": participant.name,
            "color": participant.color,
            "x": participant.x,
            "y": participant.y
        }]);
        self.server.broadcast_to_channel(channel_id, &participant_msg, Some(client_id)).await;

        self.server.broadcast_ls_update(channel_id, false).await;
    }

    async fn handle_channel_settings(&self, client_id: &str, data: &serde_json::Value) {
        let set = match data.get("set") {
            Some(s) => s,
            None => return,
        };

        let client_ref = match self.server.clients.get(client_id) {
            Some(c) => c,
            None => return,
        };
        
        let client = client_ref.value().read().await;
        let channel_id = match client.channel_id.as_ref() {
            Some(id) => id.clone(),
            None => return,
        };
        drop(client);

        let channel_ref = match self.server.channels.get(&channel_id) {
            Some(c) => c,
            None => return,
        };
        
        let mut channel = channel_ref.value().write().await;

        if let Some(crown) = &channel.crown {
            if crown.participant_id.as_deref() != Some(client_id) {
                return;
            }
        }

        if channel._id == "lobby" || channel._id.starts_with("test/") {
            return;
        }

        if let Some(color) = set.get("color").and_then(|c| c.as_str()) {
            channel.settings.color = color.to_string();
        }
        if let Some(visible) = set.get("visible").and_then(|v| v.as_bool()) {
            channel.settings.visible = visible;
        }
        if let Some(chat) = set.get("chat").and_then(|c| c.as_bool()) {
            channel.settings.chat = Some(chat);
        }
        if let Some(crownsolo) = set.get("crownsolo").and_then(|c| c.as_bool()) {
            channel.settings.crownsolo = Some(crownsolo);
        }

        let ppl: Vec<_> = channel.participants.values().cloned().collect();
        let update_msg = serde_json::json!([{
            "m": "ch",
            "ch": {
                "_id": channel._id,
                "settings": channel.settings,
                "crown": channel.crown
            },
            "ppl": ppl
        }]);

        drop(channel);
        self.server.broadcast_to_channel(&channel_id, &update_msg, None).await;
        self.server.broadcast_ls_update(&channel_id, false).await;
    }

    async fn handle_chown(&self, client_id: &str, data: &serde_json::Value) {
        let target_id = data.get("id").and_then(|id| id.as_str());

        let client_ref = match self.server.clients.get(client_id) {
            Some(c) => c,
            None => return,
        };
        
        let client = client_ref.value().read().await;
        let channel_id = match client.channel_id.as_ref() {
            Some(id) => id.clone(),
            None => return,
        };
        
        let participant = match client.participant.as_ref() {
            Some(p) => p.clone(),
            None => return,
        };
        drop(client);

        let channel_ref = match self.server.channels.get(&channel_id) {
            Some(c) => c,
            None => return,
        };
        
        let mut channel = channel_ref.value().write().await;

        if channel.settings.lobby {
            return;
        }

        let crown = match channel.crown.as_mut() {
            Some(c) => c,
            None => return,
        };
        
        if crown.participant_id.as_deref() != Some(client_id) {
            return;
        }

        if let Some(target_id) = target_id {
            if let Some(target_ref) = self.server.clients.get(target_id) {
                let target = target_ref.value().read().await;
                let target_participant = match target.participant.as_ref() {
                    Some(p) => p,
                    None => return,
                };

                *crown = Crown {
                    participant_id: Some(target_id.to_string()),
                    user_id: Some(target.user_id.clone()),
                    time: current_time_ms(),
                    start_pos: Position { x: participant.x, y: participant.y },
                    end_pos: Position { x: target_participant.x, y: target_participant.y },
                };
            }
        } else {
            *crown = Crown {
                participant_id: None,
                user_id: Some(participant._id.clone()),
                time: current_time_ms(),
                start_pos: Position { x: participant.x, y: participant.y },
                end_pos: Position { x: participant.x, y: participant.y },
            };
        }

        let ppl: Vec<_> = channel.participants.values().cloned().collect();
        let channel_update = serde_json::json!([{
            "m": "ch",
            "ch": {
                "_id": channel._id,
                "settings": channel.settings,
                "crown": channel.crown
            },
            "ppl": ppl
        }]);

        drop(channel);
        self.server.broadcast_to_channel(&channel_id, &channel_update, None).await;
    }

    async fn handle_kickban(&self, client_id: &str, data: &serde_json::Value) {
        let target_user_id = match data.get("_id").and_then(|id| id.as_str()) {
            Some(id) => id,
            None => return,
        };
        
        let duration_ms = match data.get("ms").and_then(|ms| ms.as_u64()) {
            Some(ms) => ms.min(24 * 60 * 60 * 1000),
            None => return,
        };

        let client_ref = match self.server.clients.get(client_id) {
            Some(c) => c,
            None => return,
        };
        
        let client = client_ref.value().read().await;
        let channel_id = match client.channel_id.as_ref() {
            Some(id) => id.clone(),
            None => return,
        };
        
        let client_name = match client.participant.as_ref() {
            Some(p) => p.name.clone(),
            None => return,
        };
        drop(client);

        let channel_ref = match self.server.channels.get(&channel_id) {
            Some(c) => c,
            None => return,
        };
        
        let channel = channel_ref.value().read().await;

        if channel.settings.lobby {
            return;
        }

        if let Some(crown) = &channel.crown {
            if crown.participant_id.as_deref() != Some(client_id) {
                return;
            }
        }

        drop(channel);

        let mut target_client_id = None;
        let mut target_name = String::new();
        
        for client_entry in self.server.clients.iter() {
            let c = client_entry.value().read().await;
            if c.user_id == target_user_id && c.channel_id.as_ref() == Some(&channel_id) {
                target_client_id = Some(client_entry.key().clone());
                if let Some(p) = &c.participant {
                    target_name = p.name.clone();
                }
                break;
            }
        }

        let target_client_id = match target_client_id {
            Some(id) => id,
            None => return,
        };

        let expiry = current_time_ms() + duration_ms;
        self.server.banned_users.insert(
            target_user_id.to_string(),
            crate::types::BanInfo {
                channel_id: channel_id.clone(),
                expiry,
            },
        );

        let kick_data = serde_json::json!({"_id": "test/awkward"});
        self.handle_channel(&target_client_id, &kick_data).await;

        let ban_notification = serde_json::json!([{
            "m": "notification",
            "id": format!("ban-{}", current_time_ms()),
            "title": "",
            "text": format!("You have been banned from {} for {} seconds.", channel_id, duration_ms / 1000),
            "class": "short",
            "duration": 5000
        }]);
        let msg_str = serde_json::to_string(&ban_notification).unwrap_or_default();
        self.server.send_to_client(&target_client_id, &msg_str).await;

        let text = if target_user_id == client_ref.value().read().await.user_id {
            format!("Let it be known that {} kickbanned him/her self.", client_name)
        } else {
            format!("{} banned {} for {} seconds.", client_name, target_name, duration_ms / 1000)
        };

        let broadcast_msg = serde_json::json!([{
            "m": "notification",
            "id": format!("ban-{}", current_time_ms()),
            "title": "",
            "text": text,
            "class": "short",
            "duration": 5000
        }]);
        self.server.broadcast_to_channel(&channel_id, &broadcast_msg, None).await;
    }

    async fn handle_unban(&self, client_id: &str, data: &serde_json::Value) {
        let target_user_id = match data.get("_id").and_then(|id| id.as_str()) {
            Some(id) => id,
            None => return,
        };

        let client_ref = match self.server.clients.get(client_id) {
            Some(c) => c,
            None => return,
        };
        
        let client = client_ref.value().read().await;
        let channel_id = match client.channel_id.as_ref() {
            Some(id) => id.clone(),
            None => return,
        };

        let channel_ref = match self.server.channels.get(&channel_id) {
            Some(c) => c,
            None => return,
        };
        
        let channel = channel_ref.value().read().await;

        if channel.settings.lobby {
            return;
        }

        if let Some(crown) = &channel.crown {
            if crown.participant_id.as_deref() != Some(client_id) {
                return;
            }
        }

        drop(channel);

        self.server.banned_users.remove(target_user_id);

        let notice = serde_json::json!([{
            "m": "notification",
            "id": format!("unban-{}", current_time_ms()),
            "title": "",
            "text": format!("Unbanned user {}", target_user_id),
            "class": "short",
            "duration": 5000
        }]);
        self.server.broadcast_to_channel(&channel_id, &notice, None).await;
    }

    async fn handle_devices(&self, client_id: &str, data: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
        let list = data.get("list")?;

        debug!("Devices from {}: {:?}", client_id, list);

        Some(vec![serde_json::json!({
            "m": "devices",
            "status": "received",
            "list": list
        })])
    }
}