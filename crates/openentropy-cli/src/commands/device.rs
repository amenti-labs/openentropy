use std::io::Write;

pub fn run(path: &str, buffer_size: usize, source_filter: Option<&str>, conditioning: &str) {
    let pool = super::make_pool(source_filter);
    let mode = super::parse_conditioning(conditioning);

    // Create FIFO
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
    ctrlc_handler(&path_owned);

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

fn ctrlc_handler(path: &str) {
    let path = path.to_string();
    let _ = ctrlc::set_handler(move || {
        let _ = std::fs::remove_file(&path);
        std::process::exit(0);
    });
}

mod ctrlc {
    pub fn set_handler<F: Fn() + Send + 'static>(handler: F) -> Result<(), ()> {
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
        std::thread::spawn(move || {
            let _ = handler;
        });
        Ok(())
    }

    extern "C" fn signal_handler(_: libc::c_int) {
        std::process::exit(0);
    }
}
