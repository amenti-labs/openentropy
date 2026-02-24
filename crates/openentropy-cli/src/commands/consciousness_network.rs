//! `openentropy consciousness-network` — networked multi-operator mode.
//!
//! TCP-based protocol for remote adversarial consciousness experiments.
//! One machine acts as the entropy server, collecting data and coordinating
//! the experiment. Remote operators connect and receive phase instructions
//! and real-time feedback over the network.
//!
//! ## Protocol
//!
//! 1. Server starts and waits for operator connections
//! 2. Operators connect via TCP and send their name
//! 3. Server assigns intention directions (High/Low) alternating
//! 4. Server runs trials, broadcasts results to all connected operators
//! 5. After all phases, server computes per-operator analysis
//!
//! ## Usage
//!
//! ```bash
//! # Machine 1: Start as server
//! openentropy consciousness-network --host --port 9042 --quick
//!
//! # Machine 2: Connect as operator
//! openentropy consciousness-network --connect 192.168.1.10:9042 --name alice
//! ```

use std::io::{BufRead, BufReader, Write as IoWrite};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use openentropy_core::conditioning::ConditioningMode;
use openentropy_core::consciousness::*;

use super::make_pool;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

pub struct NetworkConfig<'a> {
    /// Run as host (server) mode.
    pub host: bool,
    /// Connect to a remote host.
    pub connect: Option<&'a str>,
    /// Port for hosting.
    pub port: u16,
    /// Operator name.
    pub name: &'a str,
    /// Source filter.
    pub source_filter: Option<&'a str>,
    /// Trials per phase.
    pub trials: usize,
    /// Bits per trial.
    pub bits: usize,
    /// Trial interval ms.
    pub interval_ms: u64,
    /// Quick mode.
    pub quick: bool,
    /// Max operators to wait for.
    pub max_operators: usize,
    /// Output path.
    pub output_path: Option<&'a str>,
}

// ---------------------------------------------------------------------------
// Wire protocol messages (newline-delimited JSON)
// ---------------------------------------------------------------------------

/// Messages from server to client.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
enum ServerMsg {
    /// Welcome with assigned direction.
    Welcome {
        operator_index: usize,
        direction: String,
        total_operators: usize,
    },
    /// Phase starting.
    PhaseStart {
        phase_index: usize,
        total_phases: usize,
        direction: String,
    },
    /// Trial result broadcast.
    TrialResult {
        trial_index: usize,
        trials_total: usize,
        pooled_z: f64,
        cumulative_z: f64,
        p_value: f64,
    },
    /// Phase complete.
    PhaseComplete {
        direction: String,
        cumulative_z: f64,
        p_value: f64,
    },
    /// Experiment complete with final results.
    ExperimentComplete {
        net_z: f64,
        net_p: f64,
        dominance_z: f64,
        interpretation: String,
    },
}

/// Messages from client to server.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
enum ClientMsg {
    /// Operator identifies themselves.
    Hello { name: String },
    /// Operator acknowledges phase start.
    Ready,
}

// ---------------------------------------------------------------------------
// Host (server) mode
// ---------------------------------------------------------------------------

pub fn run(cfg: NetworkConfig<'_>) {
    if cfg.host {
        run_host(cfg);
    } else if let Some(addr) = cfg.connect {
        run_client(addr, cfg.name);
    } else {
        eprintln!("Error: specify --host to run as server, or --connect <addr> to join");
        std::process::exit(1);
    }
}

fn run_host(cfg: NetworkConfig<'_>) {
    let pool = make_pool(cfg.source_filter);
    let source_infos = pool.source_infos();

    if source_infos.is_empty() {
        eprintln!("Error: no entropy sources available");
        std::process::exit(1);
    }

    let active_sources: Vec<(String, String)> = source_infos
        .iter()
        .filter(|s| !s.composite)
        .map(|s| (s.name.clone(), s.category.clone()))
        .collect();

    let bind_addr = format!("0.0.0.0:{}", cfg.port);
    let listener = match TcpListener::bind(&bind_addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error binding to {bind_addr}: {e}");
            std::process::exit(1);
        }
    };

    println!();
    println!("  CONSCIOUSNESS-RNG NETWORK EXPERIMENT (HOST)");
    println!("  {}", "=".repeat(50));
    println!("  Listening on:  {bind_addr}");
    println!("  Sources:       {} active", active_sources.len());
    println!("  Waiting for {} operator(s)...", cfg.max_operators);
    println!();

    // Accept operator connections
    listener
        .set_nonblocking(false)
        .expect("Cannot set blocking");
    // Set a timeout for accepting connections
    let _ = listener.set_nonblocking(true);

    let operators: Arc<Mutex<Vec<(String, TcpStream)>>> = Arc::new(Mutex::new(Vec::new()));
    let accept_start = Instant::now();
    let accept_timeout = Duration::from_secs(120);

    while accept_start.elapsed() < accept_timeout {
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("  Connection from {addr}");
                let _ = stream.set_nodelay(true);

                // Read Hello message
                let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
                let mut line = String::new();
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                if let Ok(_) = reader.read_line(&mut line) {
                    if let Ok(msg) = serde_json::from_str::<ClientMsg>(&line) {
                        match msg {
                            ClientMsg::Hello { name } => {
                                println!("  Operator connected: {name}");
                                let mut ops = operators.lock().unwrap();
                                ops.push((name, reader.into_inner()));
                            }
                            _ => {}
                        }
                    }
                }

                let ops = operators.lock().unwrap();
                if ops.len() >= cfg.max_operators {
                    break;
                }
                println!(
                    "  {}/{} operators connected. Waiting...",
                    ops.len(),
                    cfg.max_operators
                );
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                eprintln!("  Accept error: {e}");
                break;
            }
        }
    }

    let mut ops = operators.lock().unwrap();
    if ops.is_empty() {
        eprintln!("  No operators connected. Aborting.");
        return;
    }

    println!("\n  Starting experiment with {} operators", ops.len());

    // Assign directions: alternate High/Low
    let directions: Vec<IntentionDirection> = ops
        .iter()
        .enumerate()
        .map(|(i, _)| {
            if i % 2 == 0 {
                IntentionDirection::High
            } else {
                IntentionDirection::Low
            }
        })
        .collect();

    // Send Welcome messages
    let n_ops = ops.len();
    for (i, (name, stream)) in ops.iter_mut().enumerate() {
        let msg = ServerMsg::Welcome {
            operator_index: i,
            direction: directions[i].to_string(),
            total_operators: n_ops,
        };
        send_msg(stream, &msg);
        println!("  {} assigned: {}", name, directions[i]);
    }

    // Run the experiment
    let trials_per_phase = if cfg.quick { 10 } else { cfg.trials };
    let phases = vec![
        IntentionDirection::Baseline,
        IntentionDirection::High,
        IntentionDirection::Low,
    ];

    let bytes_per_trial = (cfg.bits + 7) / 8;
    let n_bits = cfg.bits;
    let mut all_phase_results: Vec<PhaseResult> = Vec::new();

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .ok();

    for (phase_idx, &direction) in phases.iter().enumerate() {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        // Broadcast phase start
        let phase_msg = ServerMsg::PhaseStart {
            phase_index: phase_idx,
            total_phases: phases.len(),
            direction: direction.to_string(),
        };
        for (_, stream) in ops.iter_mut() {
            send_msg(stream, &phase_msg);
        }

        println!(
            "\n  Phase {}/{}: {}",
            phase_idx + 1,
            phases.len(),
            direction
        );

        let mut phase_trials: Vec<Trial> = Vec::new();
        let mut cumulative_zs: Vec<f64> = Vec::new();
        let experiment_start = Instant::now();

        for trial_idx in 0..trials_per_phase {
            if !running.load(Ordering::SeqCst) {
                break;
            }

            let trial_start = Instant::now();
            let timestamp_secs = experiment_start.elapsed().as_secs_f64();
            let mut source_trials: Vec<SourceTrial> = Vec::new();

            for (source_name, category) in &active_sources {
                let conditioned = pool
                    .get_source_bytes(source_name, bytes_per_trial, ConditioningMode::Sha256)
                    .unwrap_or_default();

                if conditioned.len() < bytes_per_trial {
                    continue;
                }

                let ones = count_ones_n(&conditioned, n_bits);
                let z = trial_z_score(ones, n_bits);

                source_trials.push(SourceTrial {
                    source_name: source_name.clone(),
                    category: category.clone(),
                    ones_count: ones,
                    z_score: z,
                });
            }

            if source_trials.is_empty() {
                continue;
            }

            let pooled_z = source_trials.iter().map(|st| st.z_score).sum::<f64>()
                / source_trials.len() as f64;

            cumulative_zs.push(pooled_z);
            let cum_z = stouffer_z(&cumulative_zs);
            let cum_p = z_to_p_two_tailed(cum_z);

            let trial = Trial {
                index: trial_idx,
                direction,
                source_trials,
                pooled_z,
                timestamp_secs,
            };

            // Broadcast trial result to all operators
            let trial_msg = ServerMsg::TrialResult {
                trial_index: trial_idx,
                trials_total: trials_per_phase,
                pooled_z,
                cumulative_z: cum_z,
                p_value: cum_p,
            };
            for (_, stream) in ops.iter_mut() {
                send_msg(stream, &trial_msg);
            }

            // Console feedback
            let bar_width = 20;
            let filled = ((trial_idx + 1) * bar_width) / trials_per_phase;
            let bar: String = (0..bar_width)
                .map(|i| if i < filled { '#' } else { '-' })
                .collect();
            print!(
                "\r  [{bar}] {:>3}/{trials_per_phase}  Z: {:>7}  p: {:<12}",
                trial_idx + 1,
                format_z(cum_z),
                format_p_value(cum_p)
            );
            let _ = std::io::stdout().flush();

            phase_trials.push(trial);

            let elapsed = trial_start.elapsed();
            let interval = Duration::from_millis(cfg.interval_ms);
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }

        println!();

        let phase_result = compute_phase_result(direction, &phase_trials);

        // Broadcast phase complete
        let complete_msg = ServerMsg::PhaseComplete {
            direction: direction.to_string(),
            cumulative_z: phase_result.cumulative_z,
            p_value: phase_result.p_value,
        };
        for (_, stream) in ops.iter_mut() {
            send_msg(stream, &complete_msg);
        }

        println!(
            "  {} complete: Z = {}, p = {}",
            direction,
            format_z(phase_result.cumulative_z),
            format_p_value(phase_result.p_value)
        );

        all_phase_results.push(phase_result);
    }

    // Compute adversarial results
    let high_phase = all_phase_results
        .iter()
        .find(|p| p.direction == IntentionDirection::High);
    let low_phase = all_phase_results
        .iter()
        .find(|p| p.direction == IntentionDirection::Low);

    let net_z = match (high_phase, low_phase) {
        (Some(h), Some(l)) => (h.cumulative_z + l.cumulative_z) / std::f64::consts::SQRT_2,
        _ => 0.0,
    };
    let net_p = z_to_p_two_tailed(net_z);
    let dominance_z = match (high_phase, low_phase) {
        (Some(h), Some(l)) => h.cumulative_z.abs() - l.cumulative_z.abs(),
        _ => 0.0,
    };

    let interpretation = if net_p < 0.05 {
        if net_z > 0.0 {
            "HIGH operators dominated — significant net effect"
        } else {
            "LOW operators dominated — significant net effect"
        }
    } else {
        "No significant net effect — intentions may have cancelled"
    };

    // Broadcast final results
    let final_msg = ServerMsg::ExperimentComplete {
        net_z,
        net_p,
        dominance_z,
        interpretation: interpretation.to_string(),
    };
    for (_, stream) in ops.iter_mut() {
        send_msg(stream, &final_msg);
    }

    // Print results
    println!();
    println!("  NETWORK ADVERSARIAL RESULTS");
    println!("  {}", "=".repeat(50));
    println!();

    for (i, (name, _)) in ops.iter().enumerate() {
        let phase = if directions[i] == IntentionDirection::High {
            high_phase
        } else {
            low_phase
        };
        if let Some(p) = phase {
            println!(
                "  {} ({}): Z = {}, p = {}",
                name,
                directions[i],
                format_z(p.cumulative_z),
                format_p_value(p.p_value)
            );
        }
    }

    println!();
    println!(
        "  Net Z: {}, p = {}",
        format_z(net_z),
        format_p_value(net_p)
    );
    println!("  Dominance Z: {}", format_z(dominance_z));
    println!();
    println!("  {interpretation}");

    // Save results
    if let Some(output_path) = cfg.output_path {
        let operator_names: Vec<String> = ops.iter().map(|(n, _)| n.clone()).collect();
        let result = serde_json::json!({
            "mode": "network_adversarial",
            "operators": operator_names,
            "directions": directions.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            "net_z": net_z,
            "net_p": net_p,
            "dominance_z": dominance_z,
            "interpretation": interpretation,
            "phases": all_phase_results.len(),
        });
        if let Ok(json) = serde_json::to_string_pretty(&result) {
            let _ = std::fs::write(output_path, json);
            println!("\n  Results saved to {output_path}");
        }
    }

    println!();
}

// ---------------------------------------------------------------------------
// Client (operator) mode
// ---------------------------------------------------------------------------

fn run_client(addr: &str, name: &str) {
    println!();
    println!("  CONSCIOUSNESS-RNG NETWORK EXPERIMENT (OPERATOR)");
    println!("  {}", "=".repeat(50));
    println!("  Connecting to {addr}...");

    let stream = match TcpStream::connect(addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  Failed to connect: {e}");
            std::process::exit(1);
        }
    };
    let _ = stream.set_nodelay(true);
    let mut writer = stream.try_clone().expect("clone stream");
    let reader = BufReader::new(stream);

    // Send Hello
    let hello = ClientMsg::Hello {
        name: name.to_string(),
    };
    send_msg(&mut writer, &hello);
    println!("  Connected as: {name}");
    println!();

    // Read messages from server
    for line_result in reader.lines() {
        match line_result {
            Ok(line) => {
                if line.is_empty() {
                    continue;
                }
                if let Ok(msg) = serde_json::from_str::<ServerMsg>(&line) {
                    match msg {
                        ServerMsg::Welcome {
                            operator_index,
                            direction,
                            total_operators,
                        } => {
                            println!(
                                "  Assigned: Operator #{} — direction: {direction}",
                                operator_index + 1
                            );
                            println!("  Total operators: {total_operators}");
                            println!();
                        }
                        ServerMsg::PhaseStart {
                            phase_index,
                            total_phases,
                            direction,
                        } => {
                            println!(
                                "  ━━━ Phase {}/{}: {} ━━━",
                                phase_index + 1,
                                total_phases,
                                direction
                            );
                            match direction.as_str() {
                                "BASELINE" => {
                                    println!(
                                        "  Relax. No intention. Just observe."
                                    );
                                }
                                "HIGH" => {
                                    println!("  Focus: INCREASE 1-bits. Push numbers UP.");
                                }
                                "LOW" => {
                                    println!("  Focus: DECREASE 1-bits. Push numbers DOWN.");
                                }
                                _ => {}
                            }
                            println!();
                        }
                        ServerMsg::TrialResult {
                            trial_index,
                            trials_total,
                            pooled_z,
                            cumulative_z,
                            p_value,
                        } => {
                            let bar_width = 30;
                            let center = bar_width / 2;
                            let pos = ((pooled_z / 3.0) * center as f64) as i32 + center as i32;
                            let pos = pos.clamp(0, bar_width as i32 - 1) as usize;

                            let mut bar = vec![' '; bar_width];
                            bar[center] = '|';
                            for i in center.min(pos)..=center.max(pos) {
                                bar[i] = if pooled_z > 0.0 { '=' } else { '=' };
                            }
                            bar[pos] = '#';
                            let bar_str: String = bar.into_iter().collect();

                            print!(
                                "\r  T{:>3}/{trials_total} [{bar_str}] Z:{:>7} cum:{:>7} p:{}",
                                trial_index + 1,
                                format_z(pooled_z),
                                format_z(cumulative_z),
                                format_p_value(p_value)
                            );
                            let _ = std::io::stdout().flush();
                        }
                        ServerMsg::PhaseComplete {
                            direction,
                            cumulative_z,
                            p_value,
                        } => {
                            println!();
                            println!(
                                "  {} complete: Z = {}, p = {}",
                                direction,
                                format_z(cumulative_z),
                                format_p_value(p_value)
                            );
                            println!();
                        }
                        ServerMsg::ExperimentComplete {
                            net_z,
                            net_p,
                            dominance_z,
                            interpretation,
                        } => {
                            println!();
                            println!("  EXPERIMENT COMPLETE");
                            println!("  {}", "=".repeat(50));
                            println!("  Net Z: {}", format_z(net_z));
                            println!("  Net p: {}", format_p_value(net_p));
                            println!("  Dominance Z: {}", format_z(dominance_z));
                            println!();
                            println!("  {interpretation}");
                            println!();
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("\n  Connection lost: {e}");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn send_msg<T: serde::Serialize>(stream: &mut TcpStream, msg: &T) {
    if let Ok(json) = serde_json::to_string(msg) {
        let line = format!("{json}\n");
        let _ = stream.write_all(line.as_bytes());
        let _ = stream.flush();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_msg_serializes() {
        let msg = ServerMsg::Welcome {
            operator_index: 0,
            direction: "HIGH".to_string(),
            total_operators: 2,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Welcome"));
        assert!(json.contains("HIGH"));
    }

    #[test]
    fn client_msg_serializes() {
        let msg = ClientMsg::Hello {
            name: "alice".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("alice"));
    }

    #[test]
    fn server_msg_deserializes() {
        let json = r#"{"type":"Welcome","operator_index":0,"direction":"HIGH","total_operators":2}"#;
        let msg: ServerMsg = serde_json::from_str(json).unwrap();
        match msg {
            ServerMsg::Welcome {
                operator_index,
                direction,
                total_operators,
            } => {
                assert_eq!(operator_index, 0);
                assert_eq!(direction, "HIGH");
                assert_eq!(total_operators, 2);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn network_config_defaults() {
        let cfg = NetworkConfig {
            host: true,
            connect: None,
            port: 9042,
            name: "test",
            source_filter: None,
            trials: 50,
            bits: 200,
            interval_ms: 1000,
            quick: true,
            max_operators: 2,
            output_path: None,
        };
        assert!(cfg.host);
        assert_eq!(cfg.port, 9042);
    }

    #[test]
    fn trial_result_msg_roundtrip() {
        let msg = ServerMsg::TrialResult {
            trial_index: 5,
            trials_total: 10,
            pooled_z: 1.234,
            cumulative_z: 0.567,
            p_value: 0.045,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMsg = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMsg::TrialResult {
                trial_index,
                pooled_z,
                ..
            } => {
                assert_eq!(trial_index, 5);
                assert!((pooled_z - 1.234).abs() < 0.001);
            }
            _ => panic!("Wrong variant"),
        }
    }
}
