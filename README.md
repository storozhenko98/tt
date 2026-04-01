# tt

P2P terminal table tennis. Find someone on your LAN, challenge them, play.

**[Download](https://github.com/storozhenko98/tt/releases/latest)** | **[Website](https://storozhenko98.github.io/tt)**

## Install

```bash
curl -fsSL https://storozhenko98.github.io/tt/install.sh | bash
```

Or grab the binary from [Releases](https://github.com/storozhenko98/tt/releases/latest):

- macOS (Apple Silicon): `tt-darwin-arm64`
- macOS (Intel): `tt-darwin-x64`
- Linux x64: `tt-linux-x64`

## Play

```bash
# Terminal 1
tt

# Terminal 2 (same machine or same LAN)
tt

# Optional: set your name
tt YourName
```

Players on the same network discover each other automatically via UDP broadcast. Select a player, press Enter to challenge, they press Y to accept. Game on.

## Controls

| Key | Action |
|-----|--------|
| `A` / `D` | Move paddle left / right |
| `W` | Toggle topspin |
| `S` | Toggle backspin |
| `Space` | Serve (your turn) |
| `H` | Help / rules overlay |
| `Q` | Quit |

## Rules

- First to 11, win by 2
- Miss the ball = opponent scores
- Serve alternates every 2 points (every 1 at deuce)
- Ball speeds up on each hit

## Spin

Spin makes things interesting:

- **Topspin** (`W`): ball accelerates forward, harder to return
- **Backspin** (`S`): ball decelerates, deceptive placement
- **Sidespin**: your paddle's movement at contact curves the ball
- **Angle**: hitting the edge of your paddle sends it at a sharper angle

## How It Works

- **Discovery**: UDP broadcast on port 44144 — instances find each other on the LAN
- **Networking**: Direct UDP between peers, host is authoritative for ball physics
- **Rendering**: [ratatui](https://github.com/ratatui/ratatui) with braille markers for smooth sub-character ball movement
- **Physics**: Acceleration-based paddle movement with momentum, spin-affected ball trajectory

## Building from Source

```bash
git clone https://github.com/storozhenko98/tt.git
cd tt
cargo build --release
./target/release/tt
```

Requires [Rust](https://rustup.rs/) stable.

## License

MIT
