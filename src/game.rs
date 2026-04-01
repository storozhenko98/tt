use std::collections::VecDeque;

// Table dimensions (game units)
pub const TABLE_W: f64 = 100.0;
pub const TABLE_H: f64 = 140.0;
pub const NET_Y: f64 = 70.0;

// Paddle
pub const PADDLE_W: f64 = 16.0;
pub const PADDLE_MARGIN: f64 = 10.0;
pub const PADDLE_SPEED: f64 = 70.0;

// Ball
pub const BALL_SPEED_INIT: f64 = 80.0;
pub const BALL_SPEED_MAX: f64 = 160.0;
pub const HIT_SPEEDUP: f64 = 1.05;
pub const MAX_TRAIL: usize = 5;

// Spin
pub const SPIN_CURVE: f64 = 25.0;
pub const SPIN_ACCEL: f64 = 12.0;
pub const SPIN_DECAY: f64 = 0.98;
pub const MAX_SPIN: f64 = 2.5;

// Ball visual radius (used for collision tolerance)
pub const BALL_RADIUS: f64 = 2.0;

// Scoring
pub const WIN_SCORE: u32 = 11;
pub const POINT_PAUSE: f64 = 1.5;

pub fn paddle_y(player: usize) -> f64 {
    if player == 0 {
        PADDLE_MARGIN
    } else {
        TABLE_H - PADDLE_MARGIN
    }
}

#[derive(Clone, Debug)]
pub struct Ball {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub spin_x: f64,
    pub spin_y: f64,
    pub trail: VecDeque<(f64, f64)>,
}

impl Default for Ball {
    fn default() -> Self {
        Self {
            x: TABLE_W / 2.0,
            y: NET_Y,
            vx: 0.0,
            vy: 0.0,
            spin_x: 0.0,
            spin_y: 0.0,
            trail: VecDeque::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Paddle {
    pub x: f64,
    pub vx: f64,
    pub spin_intent: i8, // -1 backspin, 0 flat, 1 topspin
}

impl Default for Paddle {
    fn default() -> Self {
        Self {
            x: TABLE_W / 2.0,
            vx: 0.0,
            spin_intent: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Serving { server: usize },
    Rally,
    Scored { winner: usize, timer: f64 },
    GameOver { winner: usize },
}

#[derive(Clone)]
pub struct Game {
    pub ball: Ball,
    pub paddles: [Paddle; 2],
    pub scores: [u32; 2],
    pub phase: Phase,
    pub rally_hits: u32,
}

impl Game {
    pub fn new() -> Self {
        Self {
            ball: Ball::default(),
            paddles: [Paddle::default(), Paddle::default()],
            scores: [0, 0],
            phase: Phase::Serving { server: 0 },
            rally_hits: 0,
        }
    }

    pub fn serve(&mut self, server: usize) {
        let dir: f64 = if server == 0 { 1.0 } else { -1.0 };
        let paddle = &self.paddles[server];
        let spin = paddle.spin_intent;

        self.ball.x = paddle.x;
        self.ball.y = paddle_y(server) + dir * 5.0;
        self.ball.vx = (paddle.x - TABLE_W / 2.0) * 0.4;
        self.ball.vy = dir * BALL_SPEED_INIT;
        self.ball.spin_x = 0.0;
        self.ball.spin_y = spin as f64 * MAX_SPIN * 0.6;
        self.ball.trail.clear();
        self.phase = Phase::Rally;
        self.rally_hits = 0;
    }

    /// Run one frame of physics. Only the host calls this.
    pub fn update(&mut self, dt: f64) {
        // Clamp paddles
        for p in &mut self.paddles {
            p.x = (p.x + p.vx * dt).clamp(PADDLE_W / 2.0, TABLE_W - PADDLE_W / 2.0);
        }

        match self.phase.clone() {
            Phase::Serving { server } => {
                // Ball tracks server's paddle
                self.ball.x = self.paddles[server].x;
                self.ball.y =
                    paddle_y(server) + if server == 0 { 5.0 } else { -5.0 };
            }
            Phase::Rally => self.step_ball(dt),
            Phase::Scored { winner, timer } => {
                let t = timer - dt;
                if t <= 0.0 {
                    self.after_point();
                } else {
                    self.phase = Phase::Scored { winner, timer: t };
                }
            }
            Phase::GameOver { .. } => {}
        }
    }

    fn step_ball(&mut self, dt: f64) {
        // Trail
        self.ball.trail.push_back((self.ball.x, self.ball.y));
        if self.ball.trail.len() > MAX_TRAIL {
            self.ball.trail.pop_front();
        }

        // Sidespin curves horizontally
        self.ball.vx += self.ball.spin_x * SPIN_CURVE * dt;

        // Topspin/backspin affects forward speed
        let sa = self.ball.spin_y * SPIN_ACCEL * dt;
        if self.ball.vy > 0.0 {
            self.ball.vy += sa;
        } else {
            self.ball.vy -= sa;
        }

        // Decay
        self.ball.spin_x *= SPIN_DECAY;
        self.ball.spin_y *= SPIN_DECAY;

        // Move
        let prev_y = self.ball.y;
        self.ball.x += self.ball.vx * dt;
        self.ball.y += self.ball.vy * dt;

        // Side walls
        if self.ball.x < 0.0 {
            self.ball.x = -self.ball.x;
            self.ball.vx = self.ball.vx.abs();
            self.ball.spin_x *= -0.7;
        } else if self.ball.x > TABLE_W {
            self.ball.x = 2.0 * TABLE_W - self.ball.x;
            self.ball.vx = -self.ball.vx.abs();
            self.ball.spin_x *= -0.7;
        }

        // Paddle 0 (bottom) — ball moving down
        let py0 = paddle_y(0);
        if self.ball.vy < 0.0 && prev_y >= py0 && self.ball.y <= py0 {
            self.paddle_hit(0);
        }

        // Paddle 1 (top) — ball moving up
        let py1 = paddle_y(1);
        if self.ball.vy > 0.0 && prev_y <= py1 && self.ball.y >= py1 {
            self.paddle_hit(1);
        }

        // Scoring
        if self.ball.y < -10.0 {
            self.award_point(1);
        } else if self.ball.y > TABLE_H + 10.0 {
            self.award_point(0);
        }
    }

    fn paddle_hit(&mut self, player: usize) {
        let paddle = &self.paddles[player];
        let half = PADDLE_W / 2.0;
        let dx = self.ball.x - paddle.x;

        if dx.abs() > half + BALL_RADIUS {
            return; // miss
        }

        let py = paddle_y(player);
        let dir: f64 = if player == 0 { 1.0 } else { -1.0 };
        self.ball.y = py + dir * (BALL_RADIUS + 1.0);

        let speed = (self.ball.vx.powi(2) + self.ball.vy.powi(2)).sqrt();
        let new_speed = (speed * HIT_SPEEDUP).min(BALL_SPEED_MAX);

        let offset = (dx / half).clamp(-1.0, 1.0);
        let angle = offset * std::f64::consts::FRAC_PI_4;

        self.ball.vx = new_speed * angle.sin() + paddle.vx * 0.25;
        self.ball.vy = dir * new_speed * angle.cos();

        // Sidespin from paddle movement
        self.ball.spin_x =
            (self.ball.spin_x + paddle.vx * 0.015).clamp(-MAX_SPIN, MAX_SPIN);

        // Topspin / backspin from intent
        self.ball.spin_y = paddle.spin_intent as f64 * MAX_SPIN;

        self.rally_hits += 1;
    }

    fn award_point(&mut self, winner: usize) {
        self.scores[winner] += 1;
        self.phase = Phase::Scored {
            winner,
            timer: POINT_PAUSE,
        };
    }

    fn after_point(&mut self) {
        let (s0, s1) = (self.scores[0], self.scores[1]);
        let max = s0.max(s1);
        let diff = (s0 as i32 - s1 as i32).abs();

        if max >= WIN_SCORE && diff >= 2 {
            self.phase = Phase::GameOver {
                winner: if s0 > s1 { 0 } else { 1 },
            };
        } else {
            // Serve alternates every 2 points; at deuce every point
            let total = s0 + s1;
            let server = if s0 >= WIN_SCORE - 1 && s1 >= WIN_SCORE - 1 {
                (total % 2) as usize
            } else {
                ((total / 2) % 2) as usize
            };
            self.phase = Phase::Serving { server };
            self.ball = Ball::default();
        }
    }
}
