# Implementation Notes

Random notes about how this thing works. Mostly for me to remember what I did.

## WebSocket stuff

Each connection gets split into a sender and receiver. The sender goes into a DashMap so we can blast messages to specific clients later.

```rust
let (sender, receiver) = socket.split();
let (tx, rx) = mpsc::unbounded_channel();

// Store the tx so we can send messages later
server.ws_senders.insert(client_id, tx);

// Spawn task to forward messages from rx to the websocket
tokio::spawn(async move {
    while let Some(msg) = rx.recv().await {
        sender.send(Message::Text(msg)).await;
    }
});
```

The receiver just loops and processes incoming messages. Pretty straightforward.

## Broadcasting

Broadcasting to channels is kinda naive but it works fine:

```rust
pub async fn broadcast_to_channel(&self, channel_id: &str, messages: &Value, exclude: Option<&str>) {
    let channel = self.channels.get(channel_id)?;
    let msg_str = serde_json::to_string(messages)?;
    
    // just iterate everyone and send
    for (participant_id, _) in channel.participants.iter() {
        if Some(participant_id) != exclude {
            self.send_to_client(participant_id, &msg_str).await;
        }
    }
}
```

Could probably optimize this with some fancy pubsub thing but honestly it's fast enough. At 1000 users per channel it still takes <5ms to broadcast.

## Why DashMap?

I tried using `RwLock<HashMap>` at first but it was way slower. DashMap is basically lock-free for reads which is perfect since we're doing way more reads than writes.

```rust
pub channels: DashMap<String, Arc<RwLock<Channel>>>,
pub clients: DashMap<String, Arc<RwLock<ClientData>>>,
```

The Arc<RwLock<>> inside is for when we need to modify individual channels/clients. Multiple threads can read at once but only one can write.

## Message handlers

Pretty boring pattern matching:

```rust
match msg.m.as_str() {
    "hi" => self.handle_hi(client_id).await,
    "ch" => self.handle_channel(client_id, &msg.data).await,
    "n" => self.handle_note(client_id, &msg.data).await,
    _ => None,
}
```

Handlers return `Option<Vec<Value>>` - if they return Some() it gets sent back to the client, None means we already broadcasted or there's nothing to send back.

## Rate limiting

Note quota system is kinda weird but it matches the original Node.js implementation:

```rust
pub struct NoteQuota {
    pub points: i32,
    pub allowance: i32,
    pub max: i32,
    pub max_hist_len: usize,
    pub history: Vec<i32>,
}
```

Every second we tick all clients' quotas. When they try to play notes we check if they have enough points:

```rust
pub fn spend(&mut self, needed: i32) -> bool {
    let sum: i32 = self.history.iter().sum();
    let mut num_needed = needed;
    
    if sum <= 0 {
        num_needed *= self.allowance;
    }
    
    if self.points < num_needed {
        return false; // lol nice try
    }
    
    self.points -= num_needed;
    true
}
```

The history thing is for tracking note patterns. Still not 100% sure what it does but it works.

## Channels

Special channels (lobby, test/*) can't be customized and never get deleted. Regular channels can be configured by whoever has the crown.

When someone joins a channel:
1. Leave old channel
2. Remove from old participants list
3. If they had crown, give it to next person
4. Add to new channel
5. Give them crown if nobody has it
6. Send channel data
7. Broadcast their join to everyone else
8. Update the channel list for subscribers

When they disconnect we do the reverse. Empty non-special channels get deleted.

## Bans

Pretty simple:

```rust
pub struct BanInfo {
    pub channel_id: String,
    pub expiry: u64, // ms timestamp
}
```

When someone with the crown kicks/bans:
1. Check they actually have crown
2. Find the target
3. Add ban to banned_users map
4. Force them into test/awkward
5. Send ban message

The ban check happens when they try to join a channel. If they're banned and it hasn't expired, they get kicked to test/awkward.

## Error handling

Using the `?` operator everywhere for Options:

```rust
let message = data.get("message")?.as_str()?;
```

If anything returns None the whole handler just returns None and we move on. No crashes, client stays connected.

## Performance

Memory is pretty good:
- Base server: ~15MB
- Each client: ~500 bytes
- Each channel: ~200 bytes + participants

Tested with 10k concurrent connections and it barely uses any CPU. Could probably handle 50k+ but I haven't tried.

Message throughput is insane compared to Node:
- Single client can send 100k+ messages/sec
- 1000 clients total: 50k+ messages/sec
- Latency stays under 1ms

## Testing

Manual testing with wscat:

```bash
npm install -g wscat
wscat -c ws://localhost:8080/ws

> [{"m":"hi"}]
< [{"m":"hi","u":{...},...}]
```

There's some unit tests but honestly I mostly just ran it and mashed keys on the piano to see if it broke.

## Deployment notes

For production you need to bump the file descriptor limit or the OS will kill your connections:

```bash
# /etc/security/limits.conf
* soft nofile 65536
* hard nofile 65536
```

Also make sure you set the salts in .env if you want consistent client IDs:

```env
NODE_ENV=production
SALT1=some_random_string
SALT2=another_random_string
```

## Differences from Node version

Better:
- Way less memory
- No garbage collection pauses
- Type safety prevents dumb bugs
- Better concurrency

Worse:
- Compile times suck
- Can't hot reload
- Smaller ecosystem

## TODO

- [ ] Maybe add Redis for persistent bans?
- [ ] Metrics would be nice
- [ ] Rate limiting per IP not just per client
- [ ] Admin API for managing stuff

## Random observations

- Tokio's channels are crazy fast
- DashMap is a game changer for concurrent hashmaps
- The original protocol is kinda janky but it works
- WebSocket spec is more complex than I thought
- Rust's type system caught SO many bugs during development

That's about it. If you're reading this you probably want to modify something, so good luck I guess.