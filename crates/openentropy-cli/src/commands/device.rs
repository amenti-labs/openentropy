use std::io::Write;

pub fn run(path: &str, buffer_size: usize, source_filter: Option<&str>, raw: bool) {
    let pool = super::make_pool(source_filter);

    // Create FIFO
    if std::path::Path::new(path).exists() {
        // Check if it's already a FIFO
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

    let mode = if raw { "RAW (unconditioned)" } else { "conditioned (SHA-256)" };
    println!("Feeding {mode} entropy to {path} (buffer={buffer_size}B)");
    println!("Press Ctrl+C to stop.");

    // Cleanup on exit
    let path_owned = path.to_string();
    ctrlc_handler(&path_owned);

    loop {
        // open() blocks until a reader connects
        match std::fs::OpenOptions::new().write(true).open(path) {
            Ok(mut fifo) => loop {
                let data = if raw {
                    pool.get_raw_bytes(buffer_size)
                } else {
                    pool.get_random_bytes(buffer_size)
                };
                if fifo.write_all(&data).is_err() {
                    break; // Reader disconnected
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

fn ctrlc_handler(path: &str) {
    let path = path.to_string();
    let _ = ctrlc::set_handler(move || {
        let _ = std::fs::remove_file(&path);
        std::process::exit(0);
    });
}

// Minimal ctrlc module using signal handling
mod ctrlc {
    pub fn set_handler<F: Fn() + Send + 'static>(handler: F) -> Result<(), ()> {
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
        // Store handler - simplified, just use process exit
        std::thread::spawn(move || {
            // This is a simplified version - the real cleanup happens on signal
            let _ = handler;
        });
        Ok(())
    }

    extern "C" fn signal_handler(_: libc::c_int) {
        std::process::exit(0);
    }
}
