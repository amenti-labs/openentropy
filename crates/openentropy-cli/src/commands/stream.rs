use std::io::Write;

use openentropy_core::conditioning::condition;

pub struct StreamArgs {
    pub positional: Vec<String>,
    pub format: String,
    pub rate: usize,
    pub bytes: usize,
    pub conditioning: String,
    pub pool: bool,
    pub all: bool,
    pub fifo: Option<String>,
}

pub fn run(args: StreamArgs) {
    if let Some(ref path) = args.fifo {
        run_fifo(path, &args);
    } else {
        run_stdout(&args);
    }
}

fn run_stdout(args: &StreamArgs) {
    let mode = super::parse_conditioning(&args.conditioning);
    let chunk_size = if args.rate > 0 {
        args.rate.min(4096)
    } else {
        4096
    };

    // Decide: single-source direct mode vs pool mode
    let use_pool = args.pool || args.all || args.positional.is_empty() || args.positional.len() > 1;

    if use_pool {
        // Pool mode: build pool from positional args, --all, or default fast sources
        let source_filter = if args.all {
            Some("all".to_string())
        } else if !args.positional.is_empty() {
            Some(args.positional.join(","))
        } else {
            None
        };
        let pool = super::make_pool(source_filter.as_deref());
        run_pool_stdout(pool, &args.format, chunk_size, args.rate, args.bytes, mode);
    } else {
        // Single-source direct mode (no pool overhead)
        let source_name = &args.positional[0];
        let source = match super::find_source(source_name) {
            Some(s) => s,
            None => {
                eprintln!(
                    "Source '{source_name}' not found. Run 'openentropy scan' to list sources."
                );
                std::process::exit(1);
            }
        };

        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        let mut total = 0usize;

        loop {
            if args.bytes > 0 && total >= args.bytes {
                break;
            }
            let want = if args.bytes == 0 {
                chunk_size
            } else {
                chunk_size.min(args.bytes - total)
            };

            let raw = source.collect(want);
            if raw.is_empty() {
                eprintln!("Warning: source '{}' returned no data", source.name());
                break;
            }
            let data = condition(&raw, want, mode);

            if write_formatted(&mut out, &data, &args.format).is_err() {
                break; // Broken pipe
            }
            let _ = out.flush();
            total += data.len();

            if args.rate > 0 {
                let sleep_dur =
                    std::time::Duration::from_secs_f64(data.len() as f64 / args.rate as f64);
                std::thread::sleep(sleep_dur);
            }
        }
    }
}

fn run_pool_stdout(
    pool: openentropy_core::EntropyPool,
    format: &str,
    chunk_size: usize,
    rate: usize,
    n_bytes: usize,
    mode: openentropy_core::conditioning::ConditioningMode,
) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut total = 0usize;

    loop {
        if n_bytes > 0 && total >= n_bytes {
            break;
        }
        let want = if n_bytes == 0 {
            chunk_size
        } else {
            chunk_size.min(n_bytes - total)
        };

        let data = pool.get_bytes(want, mode);

        if write_formatted(&mut out, &data, format).is_err() {
            break; // Broken pipe
        }
        let _ = out.flush();
        total += data.len();

        if rate > 0 {
            let sleep_dur = std::time::Duration::from_secs_f64(data.len() as f64 / rate as f64);
            std::thread::sleep(sleep_dur);
        }
    }
}

fn run_fifo(path: &str, args: &StreamArgs) {
    let source_filter = if args.all {
        Some("all".to_string())
    } else if !args.positional.is_empty() {
        Some(args.positional.join(","))
    } else {
        None
    };
    let pool = super::make_pool(source_filter.as_deref());
    let mode = super::parse_conditioning(&args.conditioning);
    let buffer_size = if args.rate > 0 { args.rate } else { 4096 };

    // Create FIFO if it doesn't exist; verify it's a FIFO if it does.
    if std::path::Path::new(path).exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            let meta = std::fs::metadata(path).unwrap();
            if !meta.file_type().is_fifo() {
                eprintln!("Error: {path} exists and is not a FIFO.");
                std::process::exit(1);
            }
        }
    } else {
        #[cfg(unix)]
        {
            use std::ffi::CString;
            let c_path = match CString::new(path) {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("Error: FIFO path contains invalid characters.");
                    std::process::exit(1);
                }
            };
            // SAFETY: c_path is a valid NUL-terminated CString.
            let ret = unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) };
            if ret != 0 {
                eprintln!("Error creating FIFO: {}", std::io::Error::last_os_error());
                std::process::exit(1);
            }
            println!("Created FIFO: {path}");
        }
        #[cfg(not(unix))]
        {
            eprintln!("Named pipes not supported on this platform.");
            std::process::exit(1);
        }
    }

    println!(
        "Feeding entropy to {path} (conditioning={}, buffer={buffer_size}B)",
        args.conditioning
    );
    println!("Press Ctrl+C to stop.");

    let path_owned = path.to_string();
    install_cleanup_handler(&path_owned);

    loop {
        match std::fs::OpenOptions::new().write(true).open(path) {
            Ok(mut fifo) => loop {
                let data = pool.get_bytes(buffer_size, mode);
                if fifo.write_all(&data).is_err() {
                    break;
                }
                let _ = fifo.flush();
            },
            Err(e) => {
                eprintln!("Error opening FIFO: {e}");
                break;
            }
        }
    }

    let _ = std::fs::remove_file(path);
}

/// Pre-computed CString for the FIFO path so the signal handler avoids heap
/// allocation (malloc is not async-signal-safe).
static FIFO_CPATH: std::sync::OnceLock<std::ffi::CString> = std::sync::OnceLock::new();

/// Register a signal handler that removes the FIFO on Ctrl+C / SIGTERM.
fn install_cleanup_handler(path: &str) {
    if let Ok(c) = std::ffi::CString::new(path) {
        let _ = FIFO_CPATH.set(c);
    }
    // SAFETY: signal() registers a C-linkage handler for SIGINT/SIGTERM.
    // signal_handler is a valid extern "C" fn with correct signature.
    unsafe {
        libc::signal(
            libc::SIGINT,
            signal_handler as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            signal_handler as *const () as libc::sighandler_t,
        );
    }
}

extern "C" fn signal_handler(_: libc::c_int) {
    // Only call async-signal-safe functions here.
    if let Some(c_path) = FIFO_CPATH.get() {
        unsafe {
            libc::unlink(c_path.as_ptr());
        }
    }
    unsafe {
        libc::_exit(0);
    }
}

fn write_formatted(out: &mut impl Write, data: &[u8], format: &str) -> std::io::Result<()> {
    match format {
        "hex" => {
            let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
            out.write_all(hex.as_bytes())
        }
        "base64" => {
            let encoded = base64_encode(data);
            out.write_all(encoded.as_bytes())
        }
        _ => out.write_all(data),
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}
