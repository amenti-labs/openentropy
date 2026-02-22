use std::io::Write;

pub fn run(
    format: &str,
    rate: usize,
    source_filter: Option<&str>,
    n_bytes: usize,
    conditioning: &str,
    fifo_path: Option<&str>,
) {
    if let Some(path) = fifo_path {
        run_fifo(path, rate, source_filter, conditioning);
    } else {
        run_stdout(format, rate, source_filter, n_bytes, conditioning);
    }
}

fn run_stdout(
    format: &str,
    rate: usize,
    source_filter: Option<&str>,
    n_bytes: usize,
    conditioning: &str,
) {
    let pool = super::make_pool(source_filter);
    let mode = super::parse_conditioning(conditioning);
    let chunk_size = if rate > 0 { rate.min(4096) } else { 4096 };
    let mut total = 0usize;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

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

        let write_result = match format {
            "raw" => out.write_all(&data),
            "hex" => {
                let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
                out.write_all(hex.as_bytes())
            }
            "base64" => {
                let encoded = base64_encode(&data);
                out.write_all(encoded.as_bytes())
            }
            _ => out.write_all(&data),
        };

        if write_result.is_err() {
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

fn run_fifo(path: &str, buffer_size: usize, source_filter: Option<&str>, conditioning: &str) {
    let pool = super::make_pool(source_filter);
    let mode = super::parse_conditioning(conditioning);
    let buffer_size = if buffer_size > 0 { buffer_size } else { 4096 };

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
            let c_path = CString::new(path).unwrap();
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

    println!("Feeding entropy to {path} (conditioning={conditioning}, buffer={buffer_size}B)");
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

/// Store the FIFO path globally so the signal handler can clean it up.
static FIFO_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Register a signal handler that removes the FIFO on Ctrl+C / SIGTERM.
fn install_cleanup_handler(path: &str) {
    let _ = FIFO_PATH.set(path.to_string());
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
    if let Some(path) = FIFO_PATH.get() {
        let _ = std::fs::remove_file(path);
    }
    std::process::exit(0);
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
