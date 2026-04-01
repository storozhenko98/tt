use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr, UdpSocket};
use std::time::Instant;

/// Get all subnet broadcast addresses for local interfaces.
fn local_broadcast_addrs() -> Vec<Ipv4Addr> {
    let mut addrs = vec![Ipv4Addr::BROADCAST]; // 255.255.255.255 always

    // Parse ifconfig output to find broadcast addresses
    if let Ok(out) = std::process::Command::new("ifconfig").output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            // macOS: "inet 192.168.1.5 netmask 0xffffff00 broadcast 192.168.1.255"
            // Linux: "inet 192.168.1.5  netmask 255.255.255.0  broadcast 192.168.1.255"
            if let Some(idx) = line.find("broadcast ") {
                let after = &line[idx + 10..];
                let token = after.split_whitespace().next().unwrap_or("");
                if let Ok(ip) = token.parse::<Ipv4Addr>() {
                    if !addrs.contains(&ip) {
                        addrs.push(ip);
                    }
                }
            }
        }
    }

    addrs
}

const DISCOVERY_PORT: u16 = 44144;
const ANNOUNCE_INTERVAL: f64 = 1.5;
const PEER_TIMEOUT: f64 = 5.0;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    // Discovery (broadcast)
    Announce {
        name: String,
        id: u64,
        game_port: u16,
    },
    // Lobby (direct)
    Challenge { name: String },
    Accept { name: String },
    Decline,
    // Gameplay (direct)
    PaddleState {
        x: f64,
        vx: f64,
        spin_intent: i8,
        serve: bool,
    },
    BallState {
        x: f64,
        y: f64,
        vx: f64,
        vy: f64,
        sx: f64,
        sy: f64,
    },
    PhaseUpdate {
        phase_code: u8, // 0=serving 1=rally 2=scored 3=gameover
        scores: [u32; 2],
        server: usize,
        winner: usize,
    },
    Quit,
}

#[derive(Clone, Debug)]
pub struct Peer {
    pub name: String,
    pub addr: SocketAddr, // game socket address
    pub last_seen: Instant,
}

pub struct Net {
    discovery: UdpSocket,
    game: UdpSocket,
    pub game_port: u16,
    pub session_id: u64,
    pub name: String,
    pub peers: HashMap<u64, Peer>,
    last_announce: Instant,
}

impl Net {
    pub fn new(name: String) -> anyhow::Result<Self> {
        use socket2::{Domain, Protocol, Socket, Type};

        // Discovery socket — shared port for broadcasts
        let disc = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        disc.set_reuse_address(true)?;
        #[cfg(target_os = "macos")]
        disc.set_reuse_port(true)?;
        disc.set_broadcast(true)?;
        disc.set_nonblocking(true)?;
        disc.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT).into())?;
        let discovery: UdpSocket = disc.into();

        // Game socket — random port for direct peer messages
        let gs = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        gs.set_nonblocking(true)?;
        gs.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())?;
        let game: UdpSocket = gs.into();
        let game_port = game.local_addr()?.port();

        let session_id: u64 = rand::random();

        Ok(Self {
            discovery,
            game,
            game_port,
            session_id,
            name,
            peers: HashMap::new(),
            last_announce: Instant::now() - std::time::Duration::from_secs(100),
        })
    }

    pub fn broadcast_announce(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_announce).as_secs_f64() < ANNOUNCE_INTERVAL {
            return;
        }
        self.last_announce = now;

        let msg = Msg::Announce {
            name: self.name.clone(),
            id: self.session_id,
            game_port: self.game_port,
        };
        if let Ok(data) = bincode::serialize(&msg) {
            // Send to all known broadcast addresses (limited + subnet-directed)
            for bcast in local_broadcast_addrs() {
                let dest = SocketAddr::from((bcast, DISCOVERY_PORT));
                let _ = self.discovery.send_to(&data, dest);
            }
        }
    }

    pub fn send_to(&self, addr: SocketAddr, msg: &Msg) {
        if let Ok(data) = bincode::serialize(msg) {
            let _ = self.game.send_to(&data, addr);
        }
    }

    /// Drain all pending messages. Discovery announces update the peer list
    /// automatically; everything else is returned for the app to handle.
    pub fn recv_all(&mut self) -> Vec<(SocketAddr, Msg)> {
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];

        // Discovery socket
        while let Ok((len, addr)) = self.discovery.recv_from(&mut buf) {
            if let Ok(Msg::Announce {
                ref name,
                id,
                game_port,
            }) = bincode::deserialize(&buf[..len])
            {
                if id != self.session_id {
                    let peer_addr = SocketAddr::new(addr.ip(), game_port);
                    self.peers.insert(
                        id,
                        Peer {
                            name: name.clone(),
                            addr: peer_addr,
                            last_seen: Instant::now(),
                        },
                    );
                }
            }
        }

        // Game socket
        while let Ok((len, addr)) = self.game.recv_from(&mut buf) {
            if let Ok(msg) = bincode::deserialize::<Msg>(&buf[..len]) {
                out.push((addr, msg));
            }
        }

        // Prune stale peers
        self.peers
            .retain(|_, p| p.last_seen.elapsed().as_secs_f64() < PEER_TIMEOUT);

        out
    }

    pub fn peer_list(&self) -> Vec<(u64, Peer)> {
        let mut v: Vec<_> = self.peers.iter().map(|(id, p)| (*id, p.clone())).collect();
        v.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        v
    }
}
