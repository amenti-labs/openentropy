use std::io::Write;

pub fn run(format: &str, rate: usize, source_filter: Option<&str>, n_bytes: usize, conditioning: &str) {
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
