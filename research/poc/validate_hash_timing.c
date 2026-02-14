// validate_hash_timing.c — Entropy source validation
// Mechanism: SHA-256 hash varying-size data (32-2048 bytes) via CommonCrypto
// Cross-correlate: compression_timing, speculative_execution
// Compile: cc -O2 -o validate_hash_timing validate_hash_timing.c -lm

#include "validate_common.h"
#include <CommonCrypto/CommonDigest.h>

static int collect_hash_timing(uint64_t *timings, int n) {
    uint64_t lcg = mach_absolute_time();
    uint8_t buf[2048];
    uint8_t digest[CC_SHA256_DIGEST_LENGTH];
    int valid = 0;

    for (int i = 0; i < n; i++) {
        int sz = 32 + (int)(lcg_next(&lcg) % 2017);
        for (int j = 0; j < sz; j++)
            buf[j] = (uint8_t)(lcg_next(&lcg) & 0xFF);

        uint64_t t0 = mach_absolute_time();
        CC_SHA256(buf, (CC_LONG)sz, digest);
        uint64_t t1 = mach_absolute_time();
        timings[valid++] = t1 - t0;
    }
    return valid;
}

// Cross-correlation: compression_timing (zlib-like computation stand-in)
static int collect_compression_cross(uint64_t *timings, int n) {
    uint64_t lcg = mach_absolute_time();
    uint8_t buf[512];
    volatile uint32_t checksum;

    for (int i = 0; i < n; i++) {
        int sz = 128 + (int)(lcg_next(&lcg) % 385);
        for (int j = 0; j < sz; j++)
            buf[j] = (uint8_t)(lcg_next(&lcg) & 0xFF);

        // Simulate computation workload similar to compression
        uint64_t t0 = mach_absolute_time();
        uint32_t cs = 0;
        for (int j = 0; j < sz; j++) {
            cs = (cs << 1) ^ buf[j];
            cs ^= (cs >> 16);
        }
        for (int r = 0; r < 50; r++)
            cs = cs * 2654435761U + 1;
        checksum = cs;
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    (void)checksum;
    return n;
}

// Cross-correlation: speculative_execution (branch-heavy workload)
static int collect_speculative_cross(uint64_t *timings, int n) {
    uint64_t lcg = mach_absolute_time();
    volatile int sink = 0;

    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();
        int acc = 0;
        for (int j = 0; j < 30; j++) {
            uint64_t v = lcg_next(&lcg);
            if (v & 1) acc += (int)(v >> 32);
            else acc -= (int)(v >> 16);
            if (v & 2) acc ^= (int)(v >> 8);
        }
        sink = acc;
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    (void)sink;
    return n;
}

int main(void) {
    print_validation_header("hash_timing");

    // === Test 1: Large sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_hash_timing(timings, LARGE_N);
    Stats s = compute_stats(timings, valid);
    printf("  Samples: %d  Mean=%.1f  StdDev=%.1f\n", valid, s.mean, s.stddev);
    printf("  Shannon=%.3f  H_inf=%.3f\n\n", s.shannon, s.min_entropy);

    // === Test 2: Autocorrelation ===
    printf("=== Test 2: Autocorrelation (lag 1-5) ===\n");
    double max_ac = 0;
    for (int lag = 1; lag <= 5; lag++) {
        double ac = autocorrelation(timings, valid, lag);
        printf("  lag-%d: %.4f%s\n", lag, ac,
               fabs(ac) > 0.5 ? " *** HIGH ***" : fabs(ac) > 0.1 ? " * warn *" : "");
        if (fabs(ac) > max_ac) max_ac = fabs(ac);
    }
    printf("\n");
    free(timings);

    // === Test 3: Stability ===
    printf("=== Test 3: Stability (%d trials x %d samples) ===\n", N_TRIALS, TRIAL_N);
    double min_ents[N_TRIALS];
    uint64_t *trial_buf = malloc(TRIAL_N * sizeof(uint64_t));
    for (int t = 0; t < N_TRIALS; t++) {
        int tv = collect_hash_timing(trial_buf, TRIAL_N);
        Stats ts = compute_stats(trial_buf, tv);
        min_ents[t] = ts.min_entropy;
        printf("  Trial %2d: H_inf=%.3f  Shannon=%.3f  N=%d\n",
               t + 1, ts.min_entropy, ts.shannon, tv);
    }
    free(trial_buf);

    double me_mean = 0, me_var = 0;
    for (int i = 0; i < N_TRIALS; i++) me_mean += min_ents[i];
    me_mean /= N_TRIALS;
    for (int i = 0; i < N_TRIALS; i++) {
        double d = min_ents[i] - me_mean;
        me_var += d * d;
    }
    double me_std = sqrt(me_var / N_TRIALS);
    printf("\n  H_inf Mean=%.3f  StdDev=%.3f\n", me_mean, me_std);
    printf("  Verdict: %s\n\n",
           me_std > 2.0 ? "UNSTABLE (std > 2.0)" :
           me_std > 1.0 ? "MARGINAL (std > 1.0)" : "STABLE");

    // === Test 4: Cross-correlation ===
    printf("=== Test 4: Cross-correlation ===\n");
    int cc_n = 5000;
    uint64_t *my_t = malloc(cc_n * sizeof(uint64_t));
    int my_v = collect_hash_timing(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    const char *cross_names[] = {"compression_timing", "speculative_execution"};
    collect_func_t cross_funcs[] = {collect_compression_cross, collect_speculative_cross};
    for (int c = 0; c < 2; c++) {
        uint64_t *other_t = malloc(cc_n * sizeof(uint64_t));
        int other_v = cross_funcs[c](other_t, cc_n);
        int use_n = cc_n < other_v ? cc_n : other_v;
        double r = pearson(my_t, other_t, use_n);
        printf("  vs %-25s: r=%.4f%s\n", cross_names[c], r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" :
               fabs(r) > 0.1 ? " * weak *" : "");
        free(other_t);
    }
    free(my_t);
    printf("\n");

    // === Summary ===
    printf("=== SUMMARY ===\n");
    printf("  H_inf (100K): %.3f\n", s.min_entropy);
    printf("  H_inf Mean (10 trials): %.3f\n", me_mean);
    printf("  H_inf StdDev: %.3f\n", me_std);
    printf("  Max autocorr: %.4f\n", max_ac);
    if (s.min_entropy < 0.5)
        printf("  VERDICT: CUT (H_inf < 0.5)\n");
    else if (me_std > 2.0)
        printf("  VERDICT: CUT (unstable, std > 2.0)\n");
    else if (s.min_entropy < 1.5 || max_ac > 0.5)
        printf("  VERDICT: DEMOTE (weak)\n");
    else
        printf("  VERDICT: KEEP\n");
    printf("\n");
    return 0;
}
