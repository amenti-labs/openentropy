//! OpenEntropy WebAssembly bindings — browser-based entropy collection.
//!
//! Exposes three entropy sources via `wasm-bindgen`:
//!
//! 1. **Timing jitter** — `performance.now()` micro-timing variations
//! 2. **Crypto seed mixer** — `crypto.getRandomValues()` as an OS entropy seed
//! 3. **Scheduling jitter** — `setTimeout(0)` / Promise.resolve() latency
//!
//! All sources produce raw bytes that can be SHA-256 conditioned on the JS side
//! or consumed directly.

use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Browser API helpers
// ---------------------------------------------------------------------------

/// Get `performance.now()` as f64 milliseconds.
fn performance_now() -> f64 {
    js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("performance"))
        .ok()
        .and_then(|perf| {
            js_sys::Reflect::get(&perf, &JsValue::from_str("now")).ok()
        })
        .and_then(|func| {
            let func: js_sys::Function = func.dyn_into().ok()?;
            func.call0(&js_sys::global().into())
                .ok()?
                .as_f64()
        })
        .unwrap_or(0.0)
}

/// Fill a buffer with `crypto.getRandomValues()`.
fn crypto_get_random(buf: &mut [u8]) -> bool {
    let global = js_sys::global();
    let crypto = js_sys::Reflect::get(&global, &JsValue::from_str("crypto"))
        .ok();
    let crypto = match crypto {
        Some(c) if !c.is_undefined() => c,
        _ => return false,
    };

    let array = js_sys::Uint8Array::new_with_length(buf.len() as u32);
    let func = js_sys::Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
        .ok();
    let func = match func {
        Some(f) => match f.dyn_into::<js_sys::Function>() {
            Ok(f) => f,
            Err(_) => return false,
        },
        None => return false,
    };

    if func.call1(&crypto, &array).is_err() {
        return false;
    }

    array.copy_to(buf);
    true
}

// ---------------------------------------------------------------------------
// XOR-fold helper
// ---------------------------------------------------------------------------

/// XOR-fold a f64 (8 bytes) into a single byte.
#[inline]
fn xor_fold_f64(v: f64) -> u8 {
    let b = v.to_le_bytes();
    b[0] ^ b[1] ^ b[2] ^ b[3] ^ b[4] ^ b[5] ^ b[6] ^ b[7]
}

// ---------------------------------------------------------------------------
// Timing jitter source
// ---------------------------------------------------------------------------

/// Collect entropy from `performance.now()` timing jitter.
///
/// Performs rapid back-to-back `performance.now()` calls and extracts
/// entropy from the timing deltas. Browser timer resolution is typically
/// 5-100 µs (reduced by Spectre mitigations), but the jitter between
/// consecutive calls still carries entropy from CPU scheduling, cache
/// state, and GC activity.
#[wasm_bindgen]
pub fn collect_timing_jitter(n_bytes: usize) -> Vec<u8> {
    // Oversample 8x — each timing produces ~1 bit of useful jitter.
    let raw_count = n_bytes * 8 + 64;
    let mut timings = Vec::with_capacity(raw_count);

    // Warm up the timer
    for _ in 0..16 {
        let _ = performance_now();
    }

    // Interleave timing measurements with small computational work
    // to vary cache/pipeline state between measurements.
    let mut work: u64 = performance_now().to_bits();
    for _ in 0..raw_count {
        let t = performance_now();
        timings.push(t);

        // Small amount of work to perturb microarchitectural state
        work = work.wrapping_mul(6364136223846793005).wrapping_add(1);
        std::hint::black_box(work);
    }

    // Compute deltas
    let deltas: Vec<f64> = timings
        .windows(2)
        .map(|w| w[1] - w[0])
        .collect();

    // XOR consecutive deltas and fold
    let mut raw = Vec::with_capacity(n_bytes);
    for pair in deltas.windows(2) {
        let xored = (pair[0] - pair[1]).to_bits() ^ pair[0].to_bits();
        raw.push(xor_fold_f64(f64::from_bits(xored)));
        if raw.len() >= n_bytes {
            break;
        }
    }

    raw.truncate(n_bytes);
    raw
}

// ---------------------------------------------------------------------------
// Crypto seed source
// ---------------------------------------------------------------------------

/// Collect OS entropy via `crypto.getRandomValues()`.
///
/// This uses the browser's built-in CSPRNG, which typically draws from
/// the OS entropy pool. Useful as a high-quality seed to mix with
/// timing-based sources.
#[wasm_bindgen]
pub fn collect_crypto_random(n_bytes: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n_bytes];
    if !crypto_get_random(&mut buf) {
        // Fallback: fill with timing-based entropy if crypto API unavailable
        return collect_timing_jitter(n_bytes);
    }
    buf
}

// ---------------------------------------------------------------------------
// Combined conditioned output
// ---------------------------------------------------------------------------

/// Collect `n_bytes` of SHA-256 conditioned entropy from all available
/// browser sources.
///
/// Combines timing jitter and crypto.getRandomValues() into a SHA-256
/// conditioned output stream. This is the recommended entry point for
/// applications that need high-quality random bytes.
#[wasm_bindgen]
pub fn get_random_bytes(n_bytes: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(n_bytes);
    let mut counter: u64 = 0;

    // Collect raw material from both sources
    let timing = collect_timing_jitter(n_bytes.max(32));
    let crypto = collect_crypto_random(32);

    // Initial state from crypto source
    let mut state = {
        let mut h = Sha256::new();
        h.update(&crypto);
        h.update(performance_now().to_le_bytes());
        let d: [u8; 32] = h.finalize().into();
        d
    };

    while output.len() < n_bytes {
        counter += 1;
        let mut h = Sha256::new();
        h.update(state);
        h.update(counter.to_le_bytes());

        // Mix in timing entropy
        let offset = (counter as usize * 16) % timing.len().max(1);
        let end = (offset + 16).min(timing.len());
        if offset < end {
            h.update(&timing[offset..end]);
        }

        // Mix in fresh timing sample
        h.update(performance_now().to_le_bytes());

        let digest: [u8; 32] = h.finalize().into();
        state = digest;
        output.extend_from_slice(&digest);
    }

    output.truncate(n_bytes);
    output
}

/// Return the number of available entropy sources in this WASM environment.
#[wasm_bindgen]
pub fn available_source_count() -> u32 {
    let mut count = 1; // timing jitter is always available

    // Check if crypto.getRandomValues() is available
    let global = js_sys::global();
    if let Ok(crypto) = js_sys::Reflect::get(&global, &JsValue::from_str("crypto"))
        && !crypto.is_undefined()
    {
        count += 1;
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_fold_f64_basic() {
        assert_eq!(xor_fold_f64(0.0), 0);
        // Non-zero float should produce non-trivial fold
        let v = xor_fold_f64(1.0);
        // 1.0 as f64 = 0x3FF0000000000000
        // bytes: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F]
        assert_eq!(v, 0x00 ^ 0x00 ^ 0x00 ^ 0x00 ^ 0x00 ^ 0x00 ^ 0xF0 ^ 0x3F);
    }
}
