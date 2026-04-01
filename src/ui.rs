use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Line as CanvasLine, Points},
        Block, Clear, Paragraph,
    },
    Frame,
};

use crate::app::{App, AppState};
use crate::game::*;

pub fn render(frame: &mut Frame, app: &App) {
    match &app.state {
        AppState::Lobby => render_lobby(frame, app),
        AppState::Challenged { from_name, .. } => render_challenge(frame, from_name),
        AppState::WaitingAccept { peer_name, .. } => render_waiting(frame, peer_name),
        AppState::Playing { .. } => render_game(frame, app),
    }
}

fn render_lobby(frame: &mut Frame, app: &App) {
    let peers = app.net.peer_list();

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  TERMINAL TABLE TENNIS",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("  You: {}", app.net.name)),
        Line::from("  Scanning for players on LAN..."),
        Line::from(""),
    ];

    if peers.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No players found yet...",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from("  Available players:"));
        lines.push(Line::from(""));
        for (i, (_, peer)) in peers.iter().enumerate() {
            let sel = i == app.lobby_selection;
            let prefix = if sel { " > " } else { "   " };
            let style = if sel {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(
                format!("{}{} ({})", prefix, peer.name, peer.addr.ip()),
                style,
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Enter] Challenge  [Q] Quit",
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(lines).block(Block::bordered().title(" tt "));
    frame.render_widget(para, frame.area());
}

fn render_challenge(frame: &mut Frame, from: &str) {
    // Render empty background first
    frame.render_widget(Block::bordered().title(" tt "), frame.area());

    let area = centered_rect(42, 8, frame.area());
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  INCOMING CHALLENGE!",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("  {} wants to play!", from)),
        Line::from(""),
        Line::from("  [Y] Accept  [N] Decline"),
    ];
    let para = Paragraph::new(lines).block(Block::bordered().title(" Challenge "));
    frame.render_widget(para, area);
}

fn render_waiting(frame: &mut Frame, name: &str) {
    frame.render_widget(Block::bordered().title(" tt "), frame.area());

    let area = centered_rect(42, 7, frame.area());
    let lines = vec![
        Line::from(""),
        Line::from(format!("  Challenging {}...", name)),
        Line::from("  Waiting for response..."),
        Line::from(""),
        Line::from("  [Esc] Cancel"),
    ];
    let para = Paragraph::new(lines).block(Block::bordered().title(" Waiting "));
    frame.render_widget(para, area);
}

fn render_game(frame: &mut Frame, app: &App) {
    let (game, is_host) = match &app.state {
        AppState::Playing { game, is_host, .. } => (game, *is_host),
        _ => return,
    };
    let local = if is_host { 0 } else { 1 };
    let remote = 1 - local;

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(2),
    ])
    .split(frame.area());

    // --- Score bar ---
    let local_name = &app.net.name;
    let remote_name = &app.opponent_name;
    let score_line = Line::from(vec![
        Span::styled(
            format!(" {}: {} ", local_name, game.scores[local]),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  vs  "),
        Span::styled(
            format!(" {}: {} ", remote_name, game.scores[remote]),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let phase_text = match &game.phase {
        Phase::Serving { server } => {
            if *server == local {
                "YOUR SERVE  [Space]".into()
            } else {
                format!("{}'s serve", remote_name)
            }
        }
        Phase::Rally => format!("Rally  ({} hits)", game.rally_hits),
        Phase::Scored { winner, .. } => {
            if *winner == local {
                "You scored!".into()
            } else {
                format!("{} scored!", remote_name)
            }
        }
        Phase::GameOver { winner } => {
            if *winner == local {
                "YOU WIN! [Q] quit".into()
            } else {
                format!("{} wins! [Q] quit", remote_name)
            }
        }
    };

    let score_para =
        Paragraph::new(vec![score_line, Line::from(format!(" {}", phase_text))])
            .block(Block::bordered());
    frame.render_widget(score_para, chunks[0]);

    // --- Game canvas ---
    let flip = local != 0; // guest sees the board flipped
    let fy = move |y: f64| -> f64 {
        if flip {
            TABLE_H - y
        } else {
            y
        }
    };

    let margin = 5.0;
    let canvas = Canvas::default()
        .block(
            Block::bordered()
                .title(" Table ")
                .style(Style::default().bg(Color::Rgb(0, 30, 0))),
        )
        .marker(Marker::Braille)
        .x_bounds([-margin, TABLE_W + margin])
        .y_bounds([-margin, TABLE_H + margin])
        .paint(move |ctx| {
            // Table border
            draw_rect(ctx, 0.0, 0.0, TABLE_W, TABLE_H, Color::DarkGray);

            // Net (dashed)
            let ny = fy(NET_Y);
            let dashes = 20;
            let seg = TABLE_W / (dashes as f64 * 2.0);
            for i in 0..dashes {
                let x1 = i as f64 * 2.0 * seg;
                ctx.draw(&CanvasLine {
                    x1,
                    y1: ny,
                    x2: x1 + seg,
                    y2: ny,
                    color: Color::White,
                });
            }

            // Center line (lengthwise)
            ctx.draw(&CanvasLine {
                x1: TABLE_W / 2.0,
                y1: 0.0,
                x2: TABLE_W / 2.0,
                y2: TABLE_H,
                color: Color::Rgb(0, 60, 0),
            });

            // Paddles
            let colors = if flip {
                [Color::Red, Color::Cyan]
            } else {
                [Color::Cyan, Color::Red]
            };
            for (i, paddle) in game.paddles.iter().enumerate() {
                let py = fy(paddle_y(i));
                let half = PADDLE_W / 2.0;
                for d in -1..=1 {
                    let yo = d as f64 * 0.8;
                    ctx.draw(&CanvasLine {
                        x1: paddle.x - half,
                        y1: py + yo,
                        x2: paddle.x + half,
                        y2: py + yo,
                        color: colors[i],
                    });
                }
            }

            // Ball trail
            if !game.ball.trail.is_empty() {
                let coords: Vec<(f64, f64)> = game
                    .ball
                    .trail
                    .iter()
                    .map(|&(x, y)| (x, fy(y)))
                    .collect();
                ctx.draw(&Points {
                    coords: &coords,
                    color: Color::Rgb(40, 40, 40),
                });
            }

            // Ball — filled oval for visibility
            let bx = game.ball.x;
            let by = fy(game.ball.y);
            for &(dy, hw) in &[
                (0.0, 2.0),
                (1.0, 1.8), (-1.0, 1.8),
                (2.0, 1.2), (-2.0, 1.2),
                (3.0, 0.4), (-3.0, 0.4),
            ] {
                ctx.draw(&CanvasLine {
                    x1: bx - hw,
                    y1: by + dy,
                    x2: bx + hw,
                    y2: by + dy,
                    color: Color::White,
                });
            }

            // Spin indicator
            let total_spin =
                game.ball.spin_x.abs() + game.ball.spin_y.abs();
            if total_spin > 0.3 {
                let sx = bx + game.ball.spin_x * 4.0;
                let raw_sy = game.ball.spin_y * 4.0;
                let sy = by + if flip { -raw_sy } else { raw_sy };
                ctx.draw(&Points {
                    coords: &[(sx, sy)],
                    color: Color::Yellow,
                });
            }
        });
    frame.render_widget(canvas, chunks[1]);

    // --- Controls ---
    let spin_text = match game.paddles[local].spin_intent {
        1 => ("[W] TOPSPIN", true),
        -1 => ("[S] BACKSPIN", true),
        _ => ("[W/S] Spin", false),
    };
    let controls = Line::from(vec![
        Span::styled(
            " [A/D] Move  ",
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            spin_text.0,
            Style::default().fg(if spin_text.1 {
                Color::Yellow
            } else {
                Color::DarkGray
            }),
        ),
        Span::styled(
            "  [Space] Serve  [H] Help  [Q] Quit",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(controls), chunks[2]);

    // --- Game over overlay ---
    if let Phase::GameOver { winner } = &game.phase {
        let you_won = *winner == local;
        let area = centered_rect(36, 9, frame.area());
        frame.render_widget(Clear, area);

        let (title, color) = if you_won {
            ("  YOU WIN!", Color::Green)
        } else {
            ("  YOU LOSE", Color::Red)
        };

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                title,
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(format!(
                "  {}  {} — {}  {}",
                local_name, game.scores[local], game.scores[remote], remote_name
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  [Q] Return to lobby",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let border_title = if you_won { " Victory " } else { " Game Over " };
        let para = Paragraph::new(lines)
            .block(Block::bordered().title(border_title))
            .style(Style::default().bg(Color::Black));
        frame.render_widget(para, area);
    }

    // --- Help overlay ---
    if app.show_help {
        render_help(frame);
    }
}

fn render_help(frame: &mut Frame) {
    let area = centered_rect(52, 22, frame.area());
    frame.render_widget(Clear, area);

    let bold = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("  RULES", bold)),
        Line::from("  First to 11 points, win by 2."),
        Line::from("  Miss the ball = opponent scores."),
        Line::from("  Serve alternates every 2 points."),
        Line::from("  At deuce (10-10+), every point."),
        Line::from("  Ball speeds up on each rally hit."),
        Line::from(""),
        Line::from(Span::styled("  CONTROLS", bold)),
        Line::from("  A / D          Move paddle"),
        Line::from("  W              Toggle topspin"),
        Line::from("  S              Toggle backspin"),
        Line::from("  Space          Serve"),
        Line::from(""),
        Line::from(Span::styled("  SPIN", bold)),
        Line::from("  Topspin:  ball speeds up"),
        Line::from("  Backspin: ball slows down"),
        Line::from("  Sidespin: paddle motion at contact"),
        Line::from("  Angle:    paddle edge = sharper"),
        Line::from(""),
        Line::from(Span::styled("  [H] close", dim)),
    ];

    let para = Paragraph::new(lines)
        .block(Block::bordered().title(" Help "))
        .style(Style::default().bg(Color::Black));
    frame.render_widget(para, area);
}

/// Draw a rectangle outline using four canvas lines.
fn draw_rect(
    ctx: &mut ratatui::widgets::canvas::Context,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: Color,
) {
    let lines = [
        (x, y, x + w, y),
        (x + w, y, x + w, y + h),
        (x + w, y + h, x, y + h),
        (x, y + h, x, y),
    ];
    for (x1, y1, x2, y2) in lines {
        ctx.draw(&CanvasLine {
            x1,
            y1,
            x2,
            y2,
            color,
        });
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
