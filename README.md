# MPP Server (Rust)

Rust port of the Multiplayer Piano server with way better performance than the Node.js version. [[Original](https://github.com/nyxikitty/quick-mpp-server)]

> **Note:** Some genius called my original README "AI slop" so I dumbed it down to their reading level. You're welcome.

https://github.com/user-attachments/assets/1e73dfa8-c40d-4217-98d6-f5d7bf7752db

## Why?

The Node.js version works fine, but it starts choking at around 1000 concurrent users. This rewrite handles 50k+ connections without breaking a sweat. Plus no garbage collection pauses, which is nice.

Performance differences:
- 3-5x faster message processing
- ~15MB memory vs ~50MB for Node
- <1ms latency even under heavy load
- 500k+ messages/sec throughput

All the same features as the original. WebSocket-based real-time piano playing, channels, chat, crown system, bans, the whole deal.

## Setup

You need Rust 1.70+ from https://rustup.rs/

```bash
git clone https://github.com/nyxikitty/mpp-server-rust.git
cd mpp-server-rust

# Sound files are already included, you're good to go

cargo build --release
```

## Running

Development:
```bash
cargo run
```

Production (much faster):
```bash
cargo run --release
```

Server runs at http://localhost:8080

## Config

Optional `.env` file:
```env
WS_PORT=8080
NODE_ENV=production
RUST_LOG=mpp_server=info
SALT1=random_string_here
SALT2=another_random_string
```

The salts are for hashing client IPs in production. If you don't set `NODE_ENV` to production, it'll just use random IDs.

## How it works

Clients connect via WebSocket at `ws://localhost:8080/ws` and send JSON arrays:

```json
[{"m": "hi"}]
[{"m": "ch", "_id": "lobby"}]
[{"m": "n", "t": 1234567890, "n": [{"n": "a1", "v": 0.5}]}]
```

### Message types

- `hi` - Connect
- `bye` - Disconnect  
- `+ls`/`-ls` - Subscribe/unsubscribe from channel list
- `t` - Time sync
- `a` - Chat
- `n` - Play notes
- `m` - Move cursor
- `userset` - Change name/color
- `ch` - Join/create channel
- `chset` - Change channel settings
- `chown` - Give crown to someone
- `kickban` - Ban user
- `unban` - Unban user
- `devices` - MIDI device list

## Code structure

```
src/
├── main.rs       - Axum setup
├── server.rs     - Connection handling
├── handlers.rs   - Message handlers
├── types.rs      - Data structures
└── utils.rs      - Helpers
client/           - HTML/CSS/JS (from original)
```

Uses Tokio for async, Axum for WebSocket, DashMap for lock-free state, Serde for JSON.

## Deployment

### systemd

`/etc/systemd/system/mpp-server.service`:
```ini
[Unit]
Description=MPP Server
After=network.target

[Service]
Type=simple
User=mpp
WorkingDirectory=/opt/mpp-server
Environment="RUST_LOG=mpp_server=info"
Environment="NODE_ENV=production"
ExecStart=/opt/mpp-server/target/release/mpp-server
Restart=always

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable mpp-server
sudo systemctl start mpp-server
```

### nginx

```nginx
server {
    listen 80;
    server_name mpp.example.com;

    location /ws {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

### Docker

```yaml
version: '3.8'

services:
  mpp-server:
    build: .
    ports:
      - "8080:8080"
    environment:
      - NODE_ENV=production
      - RUST_LOG=mpp_server=info
    restart: unless-stopped
```

`Dockerfile`:
```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY Cargo.* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/mpp-server .
COPY client ./client
EXPOSE 8080
CMD ["./mpp-server"]
```

## Troubleshooting

**WebSocket won't connect**
- Check your firewall
- Make sure you're using `ws://` not `http://`
- Look at browser console

**Notes don't play**
- Sound files should be in `client/sounds/`
- Check browser console for 404s

**Memory issues**
- Check if the banned users map is getting huge
- Use `--release` flag, debug builds use way more memory

## Performance tips

- Always use `--release` for production (it's like 10-100x faster)
- Set `RUST_LOG` to `info` or `warn` in production
- Increase file descriptor limit: `ulimit -n 65535`

## Tech stack

- tokio - async runtime
- axum - web framework
- serde - JSON
- dashmap - concurrent hashmap
- tower-http - middleware
- tracing - logging

## License

MIT - same as the original

## Contributing

PRs welcome. Just try to keep it compatible with the Node version's protocol.

## Credits

Based on the original MPP server

Piano sounds from https://github.com/multiplayerpiano/piano-sounds