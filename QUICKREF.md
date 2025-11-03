# Quick Reference

## Project Structure
```
mpp-server/
├── src/
│   ├── main.rs          # Entry point & Axum server setup
│   ├── server.rs        # Core server with connection management
│   ├── handlers.rs      # All message type handlers
│   ├── types.rs         # Data structures (Channel, Client, etc.)
│   └── utils.rs         # Helper functions (ID generation, time)
├── tests/
│   └── client_test.rs   # Integration tests
├── client/              # (Copy from original Node.js server)
├── Cargo.toml           # Dependencies
├── Dockerfile           # Container build
├── docker-compose.yml   # Easy deployment
└── run.sh              # Quick start script
```

## Quick Commands

```bash
# Development
cargo run

# Production build
cargo build --release
./target/release/mpp-server

# With Docker
docker-compose up -d

# Run tests
cargo test

# Format code
cargo fmt

# Check for issues
cargo clippy
```

## Environment Variables

```bash
WS_PORT=8080              # Server port
NODE_ENV=development      # or "production"
SALT1=random_string       # For client ID hashing (prod only)
SALT2=random_string       # For client ID hashing (prod only)
RUST_LOG=mpp_server=debug # Logging level
```

## Message Protocol Cheat Sheet

### Client → Server

| Message | Purpose | Data |
|---------|---------|------|
| `hi` | Handshake | None |
| `bye` | Disconnect | None |
| `+ls` | Subscribe to channel list | None |
| `-ls` | Unsubscribe from channel list | None |
| `t` | Time sync | `e`: echo value |
| `a` | Chat | `message`: string |
| `n` | Play notes | `n`: array, `t`: timestamp |
| `m` | Move cursor | `x`: float, `y`: float |
| `userset` | Change name/color | `set`: {name, color} |
| `ch` | Join channel | `_id`: channel name |
| `chset` | Change channel settings | `set`: {settings} |
| `chown` | Transfer crown | `id`: target user (optional) |
| `kickban` | Ban user | `_id`: user ID, `ms`: duration |
| `unban` | Unban user | `_id`: user ID |
| `devices` | MIDI devices | `list`: array |

### Server → Client

| Message | Purpose | Data |
|---------|---------|------|
| `hi` | Handshake response | User data, MOTD |
| `nq` | Note quota params | Quota settings |
| `t` | Time response | `t`: timestamp, `e`: echo |
| `a` | Chat message | `a`: text, `p`: participant |
| `n` | Note played | `n`: notes, `p`: player ID |
| `m` | Cursor moved | `id`: user, `x`, `y` |
| `p` | Participant update | User data |
| `bye` | User left | `p`: user ID |
| `ch` | Channel data | Channel info, participants |
| `c` | Chat history | `c`: message array |
| `ls` | Channel list | `u`: channels array |
| `notification` | Alert message | `text`, `class`, `duration` |

## Common Tasks

### Test Connection
```bash
wscat -c ws://localhost:8080/ws
> [{"m":"hi"}]
< [{"m":"hi",...}]
```

### Join a Channel
```json
[{"m":"ch","_id":"lobby"}]
```

### Play Notes
```json
[{
  "m":"n",
  "t":1234567890,
  "n":[
    {"n":"a1","v":0.5},
    {"n":"c2","v":0.7}
  ]
}]
```

### Chat
```json
[{"m":"a","message":"Hello!"}]
```

### Ban Someone (if you have crown)
```json
[{
  "m":"kickban",
  "_id":"user_id_here",
  "ms":60000
}]
```

## Performance Tuning

### Increase System Limits
```bash
# For many connections
ulimit -n 65536

# Or in /etc/security/limits.conf
* soft nofile 65536
* hard nofile 65536
```

### Optimize Rust Build
```bash
# Already in Cargo.toml:
# - opt-level = 3
# - lto = true
# - codegen-units = 1
```

### Monitor Performance
```bash
# CPU usage
top -p $(pgrep mpp-server)

# Memory usage
ps aux | grep mpp-server

# Network connections
ss -tuln | grep 8080
```

## Debugging

### Enable Debug Logs
```bash
RUST_LOG=mpp_server=debug,tower_http=debug cargo run
```

### Check WebSocket Connection
```javascript
// In browser console
const ws = new WebSocket('ws://localhost:8080/ws');
ws.onopen = () => console.log('Connected');
ws.onmessage = (e) => console.log('Message:', e.data);
ws.send(JSON.stringify([{m: 'hi'}]));
```

### Common Issues

**Port already in use**
```bash
# Find process
lsof -i :8080
# Kill it
kill -9 <PID>
```

**Rust not found**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

**Client files missing**
```bash
# Copy from original server
cp -r /path/to/node-server/client ./
```

## API Endpoints

| Endpoint | Type | Purpose |
|----------|------|---------|
| `/ws` | WebSocket | Main protocol |
| `/` | HTTP GET | Client interface |
| `/*` | HTTP GET | Static files from client/ |

## Architecture Overview

```
┌─────────────┐
│   Client    │
│  (Browser)  │
└──────┬──────┘
       │ WebSocket
       ▼
┌─────────────────────────────┐
│     Axum Web Server         │
│   ┌─────────────────────┐   │
│   │  WebSocket Handler  │   │
│   └─────────┬───────────┘   │
│             ▼               │
│   ┌─────────────────────┐   │
│   │  Message Handler    │   │
│   │  - Parse messages   │   │
│   │  - Route to funcs   │   │
│   └─────────┬───────────┘   │
│             ▼               │
│   ┌─────────────────────┐   │
│   │   Server State      │   │
│   │  - Clients (DashMap)│   │
│   │  - Channels         │   │
│   │  - WS Senders       │   │
│   └─────────┬───────────┘   │
│             ▼               │
│   ┌─────────────────────┐   │
│   │   Broadcast Logic   │   │
│   │  - To channel       │   │
│   │  - To client        │   │
│   │  - To subscribers   │   │
│   └─────────────────────┘   │
└─────────────────────────────┘
```

## Data Flow

```
Incoming Message
  ↓
Parse JSON
  ↓
Match message type ("m" field)
  ↓
Call appropriate handler
  ↓
Read/Write server state
  ↓
Generate response(s)
  ↓
Send via WebSocket channel
  ↓
Outgoing to client(s)
```

## Concurrency Model

```
Main Thread
  ├─ Axum HTTP Server
  ├─ WebSocket Listener
  │   └─ Per-Connection Tasks
  │       ├─ Incoming Message Handler
  │       └─ Outgoing Message Sender
  └─ Background Tasks
      └─ Note Quota Ticker (1/sec)
```

## Key Data Structures

```rust
Server {
    channels: DashMap<String, Arc<RwLock<Channel>>>,
    clients: DashMap<String, Arc<RwLock<ClientData>>>,
    ws_senders: DashMap<String, mpsc::Sender>,
    subscribed_to_ls: DashMap<String, bool>,
    banned_users: DashMap<String, BanInfo>,
}

Channel {
    _id: String,
    settings: ChannelSettings,
    crown: Option<Crown>,
    participants: HashMap<String, Participant>,
    chat_history: Vec<ChatMessage>,
}

ClientData {
    user_id: String,
    participant: Option<Participant>,
    channel_id: Option<String>,
    last_move_time: Option<u64>,
    note_quota: NoteQuota,
}
```

## Testing Checklist

- [ ] Server starts without errors
- [ ] Client can connect via WebSocket
- [ ] Hi message returns user data
- [ ] Can join lobby channel
- [ ] Can join custom channel
- [ ] Notes play and broadcast
- [ ] Chat messages work
- [ ] Cursor movement broadcasts
- [ ] Crown system works
- [ ] Ban/kick functions work
- [ ] Rate limiting prevents spam
- [ ] Multiple clients can interact
- [ ] Disconnection cleans up properly

## Production Checklist

- [ ] Set NODE_ENV=production
- [ ] Set unique SALT1 and SALT2
- [ ] Use --release build
- [ ] Configure reverse proxy
- [ ] Set up systemd service
- [ ] Increase system limits
- [ ] Enable logging
- [ ] Set up monitoring
- [ ] Configure firewall
- [ ] Test under load

## Resources

- **Original Server**: https://github.com/nyxikitty/quick-mpp-server
- **Rust**: https://rust-lang.org
- **Tokio**: https://tokio.rs
- **Axum**: https://docs.rs/axum
- **WebSocket Protocol**: RFC 6455

---

**Note**: This is a complete, working implementation. All core features are functional!