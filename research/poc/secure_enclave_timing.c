// Secure Enclave (SEP) round-trip timing jitter
// Keychain operations go through the Secure Enclave Processor, which is a
// separate chip with its own clock domain. The round-trip time varies due to:
// - SEP internal scheduling
// - Bus contention between AP and SEP
// - SEP power state (sleep/wake transitions)
// - SEP internal cache state
// This is genuinely novel — no prior work has exploited SEP timing for entropy.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <Security/Security.h>
#include <CoreFoundation/CoreFoundation.h>

#define N_SAMPLES 15000

int main(void) {
    printf("# Secure Enclave Round-Trip Timing Jitter\n");
    printf("# Measuring keychain operation timing through SEP...\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    uint64_t timings[N_SAMPLES];

    // Use SecRandomCopyBytes which goes through the SEP on Apple Silicon
    // This is NOT using it as an RNG — we're measuring the TIME it takes,
    // which reflects SEP scheduling/bus state, an independent entropy domain.
    uint8_t buf[32];

    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        // SecRandomCopyBytes routes through SEP on Apple Silicon
        SecRandomCopyBytes(kSecRandomDefault, sizeof(buf), buf);
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }

    // Compute deltas
    uint64_t deltas[N_SAMPLES];
    int n_deltas = 0;
    for (int i = 1; i < N_SAMPLES; i++) {
        deltas[n_deltas++] = timings[i] - timings[i-1];
    }

    // Analyze raw timing LSBs
    int hist[256] = {0};
    for (int i = 0; i < N_SAMPLES; i++) {
        hist[timings[i] & 0xFF]++;
    }

    double shannon = 0.0;
    int max_count = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            double p = (double)hist[i] / N_SAMPLES;
            shannon -= p * log2(p);
        }
        if (hist[i] > max_count) max_count = hist[i];
    }
    double min_entropy = -log2((double)max_count / N_SAMPLES);

    // Analyze XOR-folded timings (all 8 bytes folded to 1)
    int xor_hist[256] = {0};
    for (int i = 0; i < N_SAMPLES; i++) {
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) {
            folded ^= (timings[i] >> (b * 8)) & 0xFF;
        }
        xor_hist[folded]++;
    }

    double xor_shannon = 0.0;
    int xor_max = 0;
    for (int i = 0; i < 256; i++) {
        if (xor_hist[i] > 0) {
            double p = (double)xor_hist[i] / N_SAMPLES;
            xor_shannon -= p * log2(p);
        }
        if (xor_hist[i] > xor_max) xor_max = xor_hist[i];
    }
    double xor_min_entropy = -log2((double)xor_max / N_SAMPLES);

    // Delta analysis
    int delta_hist[256] = {0};
    for (int i = 0; i < n_deltas; i++) {
        delta_hist[deltas[i] & 0xFF]++;
    }
    double delta_shannon = 0.0;
    int delta_max = 0;
    for (int i = 0; i < 256; i++) {
        if (delta_hist[i] > 0) {
            double p = (double)delta_hist[i] / n_deltas;
            delta_shannon -= p * log2(p);
        }
        if (delta_hist[i] > delta_max) delta_max = delta_hist[i];
    }
    double delta_min_entropy = -log2((double)delta_max / n_deltas);

    // XOR consecutive timing pairs
    int xor_pair_hist[256] = {0};
    int n_xor = 0;
    for (int i = 0; i + 1 < N_SAMPLES; i += 2) {
        uint64_t x = timings[i] ^ timings[i+1];
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (x >> (b*8)) & 0xFF;
        xor_pair_hist[folded]++;
        n_xor++;
    }
    double xp_shannon = 0.0;
    int xp_max = 0;
    for (int i = 0; i < 256; i++) {
        if (xor_pair_hist[i] > 0) {
            double p = (double)xor_pair_hist[i] / n_xor;
            xp_shannon -= p * log2(p);
        }
        if (xor_pair_hist[i] > xp_max) xp_max = xor_pair_hist[i];
    }
    double xp_min_entropy = -log2((double)xp_max / n_xor);

    // Stats
    uint64_t sum = 0, min = UINT64_MAX, max = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        sum += timings[i];
        if (timings[i] < min) min = timings[i];
        if (timings[i] > max) max = timings[i];
    }
    double mean = (double)sum / N_SAMPLES;
    double variance = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        double d = timings[i] - mean;
        variance += d * d;
    }
    variance /= N_SAMPLES;

    uint64_t mean_ns = (uint64_t)(mean * tb.numer / tb.denom);

    printf("SEP Round-Trip Timing:\n");
    printf("  Samples: %d\n", N_SAMPLES);
    printf("  Mean: %.1f ticks (≈%llu ns)\n", mean, mean_ns);
    printf("  Range: %llu - %llu ticks\n", min, max);
    printf("  StdDev: %.1f ticks\n", sqrt(variance));
    printf("\n");
    printf("  Raw LSB:         Shannon=%.3f  H∞=%.3f\n", shannon, min_entropy);
    printf("  XOR-folded:      Shannon=%.3f  H∞=%.3f\n", xor_shannon, xor_min_entropy);
    printf("  Delta LSB:       Shannon=%.3f  H∞=%.3f\n", delta_shannon, delta_min_entropy);
    printf("  XOR-pair folded: Shannon=%.3f  H∞=%.3f\n", xp_shannon, xp_min_entropy);

    return 0;
}
