use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: String,
    pub _id: String,
    pub name: String,
    pub color: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crown {
    #[serde(rename = "participantId")]
    pub participant_id: Option<String>,
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
    pub time: u64,
    #[serde(rename = "startPos")]
    pub start_pos: Position,
    #[serde(rename = "endPos")]
    pub end_pos: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSettings {
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color2: Option<String>,
    pub lobby: bool,
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crownsolo: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub _id: String,
    pub settings: ChannelSettings,
    pub crown: Option<Crown>,
    pub participants: HashMap<String, Participant>,
    pub chat_history: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub m: String,
    pub a: String,
    pub p: Participant,
    pub t: u64,
}

#[derive(Debug, Clone)]
pub struct ClientData {
    pub user_id: String,
    pub participant: Option<Participant>,
    pub channel_id: Option<String>,
    pub last_move_time: Option<u64>,
    pub note_quota: NoteQuota,
}

#[derive(Debug, Clone)]
pub struct NoteQuota {
    pub points: i32,
    pub allowance: i32,
    pub max: i32,
    pub max_hist_len: usize,
    pub history: Vec<i32>,
}

impl NoteQuota {
    pub fn new() -> Self {
        let max = 24000;
        let max_hist_len = 3;
        let mut history = Vec::new();
        for _ in 0..max_hist_len {
            history.push(max);
        }
        
        Self {
            points: max,
            allowance: 8000,
            max,
            max_hist_len,
            history,
        }
    }

    pub fn tick(&mut self) {
        self.history.insert(0, self.points);
        self.history.truncate(self.max_hist_len);

        if self.points < self.max {
            self.points += self.allowance;
            if self.points > self.max {
                self.points = self.max;
            }
        }
    }

    pub fn spend(&mut self, needed: i32) -> bool {
        let sum: i32 = self.history.iter().sum();
        let mut num_needed = needed;

        if sum <= 0 {
            num_needed *= self.allowance;
        }

        if self.points < num_needed {
            return false;
        }

        self.points -= num_needed;
        true
    }

    pub fn get_params(&self) -> serde_json::Value {
        serde_json::json!({
            "m": "nq",
            "allowance": self.allowance,
            "max": self.max,
            "maxHistLen": self.max_hist_len
        })
    }
}

#[derive(Debug, Clone)]
pub struct BanInfo {
    pub channel_id: String,
    pub expiry: u64,
}

// Message types
#[derive(Debug, Deserialize)]
pub struct IncomingMessage {
    pub m: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
}