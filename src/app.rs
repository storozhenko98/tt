use std::net::SocketAddr;
use std::time::Instant;

use crossterm::event::KeyCode;

use crate::game::*;
use crate::net::*;

const KEY_HOLD_MS: u128 = 300;

#[derive(Clone)]
pub enum AppState {
    Lobby,
    WaitingAccept {
        peer_addr: SocketAddr,
        peer_name: String,
    },
    Challenged {
        from_addr: SocketAddr,
        from_name: String,
    },
    Playing {
        game: Game,
        is_host: bool,
        peer_addr: SocketAddr,
    },
}

pub struct App {
    pub state: AppState,
    pub net: Net,
    pub should_quit: bool,
    pub lobby_selection: usize,
    pub opponent_name: String,
    // Track held keys via "last seen" timestamps
    left_at: Option<Instant>,
    right_at: Option<Instant>,
    // Deferred serve flag — set by key_game / process_network,
    // consumed in tick_game so the phase change is visible to diff logic.
    pending_serve: bool,
    pub show_help: bool,
}

impl App {
    pub fn new(name: String) -> anyhow::Result<Self> {
        Ok(Self {
            state: AppState::Lobby,
            net: Net::new(name)?,
            should_quit: false,
            lobby_selection: 0,
            opponent_name: String::new(),
            left_at: None,
            right_at: None,
            pending_serve: false,
            show_help: false,
        })
    }

    // ── input ───────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyCode) {
        match &self.state {
            AppState::Lobby => self.key_lobby(key),
            AppState::WaitingAccept { .. } => {
                if matches!(key, KeyCode::Esc) {
                    self.state = AppState::Lobby;
                }
            }
            AppState::Challenged { .. } => self.key_challenge(key),
            AppState::Playing { .. } => self.key_game(key),
        }
    }

    fn key_lobby(&mut self, key: KeyCode) {
        let n = self.net.peer_list().len();
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.lobby_selection > 0 {
                    self.lobby_selection -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if n > 0 && self.lobby_selection < n - 1 {
                    self.lobby_selection += 1;
                }
            }
            KeyCode::Enter => {
                let peers = self.net.peer_list();
                if let Some((_, peer)) = peers.get(self.lobby_selection) {
                    let addr = peer.addr;
                    let name = peer.name.clone();
                    self.net
                        .send_to(addr, &Msg::Challenge { name: self.net.name.clone() });
                    self.state = AppState::WaitingAccept {
                        peer_addr: addr,
                        peer_name: name,
                    };
                }
            }
            _ => {}
        }
    }

    fn key_challenge(&mut self, key: KeyCode) {
        let (addr, name) = match &self.state {
            AppState::Challenged {
                from_addr,
                from_name,
            } => (*from_addr, from_name.clone()),
            _ => return,
        };
        match key {
            KeyCode::Char('y') | KeyCode::Enter => {
                self.net
                    .send_to(addr, &Msg::Accept { name: self.net.name.clone() });
                self.opponent_name = name;
                self.state = AppState::Playing {
                    game: Game::new(),
                    is_host: false,
                    peer_addr: addr,
                };
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.net.send_to(addr, &Msg::Decline);
                self.state = AppState::Lobby;
            }
            _ => {}
        }
    }

    fn key_game(&mut self, key: KeyCode) {
        // Extract local player index
        let (is_host, peer_addr) = match &self.state {
            AppState::Playing {
                is_host, peer_addr, ..
            } => (*is_host, *peer_addr),
            _ => return,
        };
        let local = if is_host { 0usize } else { 1 };

        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.net.send_to(peer_addr, &Msg::Quit);
                self.state = AppState::Lobby;
            }
            KeyCode::Left | KeyCode::Char('a') => {
                self.left_at = Some(Instant::now());
            }
            KeyCode::Right | KeyCode::Char('d') => {
                self.right_at = Some(Instant::now());
            }
            KeyCode::Char('h') => {
                self.show_help = !self.show_help;
            }
            KeyCode::Char('w') => {
                if let AppState::Playing { game, .. } = &mut self.state {
                    let p = &mut game.paddles[local];
                    p.spin_intent = if p.spin_intent == 1 { 0 } else { 1 };
                }
            }
            KeyCode::Char('s') => {
                if let AppState::Playing { game, .. } = &mut self.state {
                    let p = &mut game.paddles[local];
                    p.spin_intent = if p.spin_intent == -1 { 0 } else { -1 };
                }
            }
            KeyCode::Char(' ') => {
                if let AppState::Playing { game, is_host, .. } = &mut self.state {
                    if let Phase::Serving { server } = game.phase {
                        if server == local {
                            if *is_host {
                                // Defer to tick_game so phase-change diff works
                                self.pending_serve = true;
                            } else {
                                // Ask host to serve for us
                                self.net.send_to(
                                    peer_addr,
                                    &Msg::PaddleState {
                                        x: game.paddles[local].x,
                                        vx: game.paddles[local].vx,
                                        spin_intent: game.paddles[local].spin_intent,
                                        serve: true,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // ── tick ─────────────────────────────────────────────────────

    pub fn tick(&mut self, dt: f64) {
        self.process_network();

        match &self.state {
            AppState::Lobby | AppState::WaitingAccept { .. } | AppState::Challenged { .. } => {
                self.net.broadcast_announce();
            }
            AppState::Playing { .. } => {
                self.tick_game(dt);
            }
        }
    }

    fn process_network(&mut self) {
        let messages = self.net.recv_all();

        for (addr, msg) in messages {
            match (&self.state, &msg) {
                (AppState::Lobby, Msg::Challenge { name }) => {
                    self.state = AppState::Challenged {
                        from_addr: addr,
                        from_name: name.clone(),
                    };
                }
                (AppState::WaitingAccept { peer_addr, .. }, Msg::Accept { name })
                    if addr == *peer_addr =>
                {
                    self.opponent_name = name.clone();
                    let pa = *peer_addr;
                    self.state = AppState::Playing {
                        game: Game::new(),
                        is_host: true,
                        peer_addr: pa,
                    };
                }
                (AppState::WaitingAccept { peer_addr, .. }, Msg::Decline)
                    if addr == *peer_addr =>
                {
                    self.state = AppState::Lobby;
                }
                (
                    AppState::Playing { .. },
                    Msg::PaddleState {
                        x,
                        vx,
                        spin_intent,
                        serve,
                    },
                ) => {
                    let x = *x;
                    let vx = *vx;
                    let spin_intent = *spin_intent;
                    let serve = *serve;
                    if let AppState::Playing { game, is_host, .. } = &mut self.state {
                        let remote = if *is_host { 1 } else { 0 };
                        game.paddles[remote].x = x;
                        game.paddles[remote].vx = vx;
                        game.paddles[remote].spin_intent = spin_intent;
                    }
                    // Defer serve to tick_game so phase-change diff works
                    if serve {
                        self.pending_serve = true;
                    }
                }
                (
                    AppState::Playing { .. },
                    Msg::BallState {
                        x,
                        y,
                        vx,
                        vy,
                        sx,
                        sy,
                    },
                ) => {
                    let (x, y, vx, vy, sx, sy) = (*x, *y, *vx, *vy, *sx, *sy);
                    if let AppState::Playing { game, is_host, .. } = &mut self.state {
                        if !*is_host {
                            game.ball.trail.push_back((game.ball.x, game.ball.y));
                            if game.ball.trail.len() > MAX_TRAIL {
                                game.ball.trail.pop_front();
                            }
                            game.ball.x = x;
                            game.ball.y = y;
                            game.ball.vx = vx;
                            game.ball.vy = vy;
                            game.ball.spin_x = sx;
                            game.ball.spin_y = sy;
                        }
                    }
                }
                (
                    AppState::Playing { .. },
                    Msg::PhaseUpdate {
                        phase_code,
                        scores,
                        server,
                        winner,
                    },
                ) => {
                    let (pc, sc, sv, wn) = (*phase_code, *scores, *server, *winner);
                    if let AppState::Playing { game, is_host, .. } = &mut self.state {
                        if !*is_host {
                            game.scores = sc;
                            let was_serving = matches!(game.phase, Phase::Serving { .. });
                            game.phase = match pc {
                                0 => Phase::Serving { server: sv },
                                1 => Phase::Rally,
                                2 => Phase::Scored {
                                    winner: wn,
                                    timer: POINT_PAUSE,
                                },
                                3 => Phase::GameOver { winner: wn },
                                _ => game.phase.clone(),
                            };
                            // Only clear trail when first entering Serving;
                            // ball position is driven by BallState messages.
                            if pc == 0 && !was_serving {
                                game.ball.trail.clear();
                            }
                        }
                    }
                }
                (AppState::Playing { .. }, Msg::Quit) => {
                    self.state = AppState::Lobby;
                }
                _ => {}
            }
        }
    }

    fn tick_game(&mut self, dt: f64) {
        let (game, is_host, peer_addr) = match &mut self.state {
            AppState::Playing {
                game,
                is_host,
                peer_addr,
            } => (game, *is_host, *peer_addr),
            _ => return,
        };
        let local = if is_host { 0 } else { 1 };

        // Smooth paddle movement with momentum
        let now = Instant::now();
        let left = self
            .left_at
            .map(|t| now.duration_since(t).as_millis() < KEY_HOLD_MS)
            .unwrap_or(false);
        let right = self
            .right_at
            .map(|t| now.duration_since(t).as_millis() < KEY_HOLD_MS)
            .unwrap_or(false);

        let target = match (left, right) {
            (true, false) => -PADDLE_SPEED,
            (false, true) => PADDLE_SPEED,
            _ => 0.0,
        };

        let vx = &mut game.paddles[local].vx;
        if target.abs() > 0.1 {
            // Snap toward target — fast attack, frame-rate independent
            let t = 1.0 - (-18.0 * dt).exp();
            *vx += (target - *vx) * t;
        } else {
            // Coast to stop — enough momentum to glide through OS key-repeat gap
            let decay = (-3.0 * dt).exp();
            *vx *= decay;
            if vx.abs() < 1.0 {
                *vx = 0.0;
            }
        }

        if is_host {
            // Consume deferred serve BEFORE capturing prev_phase
            if self.pending_serve {
                self.pending_serve = false;
                if let Phase::Serving { server } = game.phase {
                    game.serve(server);
                }
            }

            game.update(dt);

            // Send ball state
            self.net.send_to(
                peer_addr,
                &Msg::BallState {
                    x: game.ball.x,
                    y: game.ball.y,
                    vx: game.ball.vx,
                    vy: game.ball.vy,
                    sx: game.ball.spin_x,
                    sy: game.ball.spin_y,
                },
            );

            // Always send phase (robust against UDP packet loss)
            let (pc, sv, wn) = match &game.phase {
                Phase::Serving { server } => (0u8, *server, 0),
                Phase::Rally => (1, 0, 0),
                Phase::Scored { winner, .. } => (2, 0, *winner),
                Phase::GameOver { winner } => (3, 0, *winner),
            };
            self.net.send_to(
                peer_addr,
                &Msg::PhaseUpdate {
                    phase_code: pc,
                    scores: game.scores,
                    server: sv,
                    winner: wn,
                },
            );
        } else {
            // Guest: only update own paddle position
            let p = &mut game.paddles[local];
            p.x = (p.x + p.vx * dt).clamp(PADDLE_W / 2.0, TABLE_W - PADDLE_W / 2.0);

            // Track ball to paddle during serve
            if let Phase::Serving { server } = game.phase {
                if server == local {
                    game.ball.x = game.paddles[local].x;
                    game.ball.y =
                        paddle_y(local) + if local == 0 { 5.0 } else { -5.0 };
                }
            }
        }

        // Always send own paddle
        self.net.send_to(
            peer_addr,
            &Msg::PaddleState {
                x: game.paddles[local].x,
                vx: game.paddles[local].vx,
                spin_intent: game.paddles[local].spin_intent,
                serve: false,
            },
        );
    }
}
