// validate_compression_timing.c — Entropy source validation
// Mechanism: Compress varying-size data (128-512 bytes, mixed patterns) with zlib
// Cross-correlate: hash_timing, amx_timing
// Compile: cc -O2 -o validate_compression_timing validate_compression_timing.c -lz -lm

#include "validate_common.h"
#include <zlib.h>

static int collect_compression_timing(uint64_t *timings, int n) {
    uint64_t lcg = mach_absolute_time();
    uint8_t src[512];
    uint8_t dst[1024];
    int valid = 0;

    for (int i = 0; i < n; i++) {
        // Vary size between 128 and 512
        int sz = 128 + (int)(lcg_next(&lcg) % 385);
        // Fill with mix of random and repeating patterns
        for (int j = 0; j < sz; j++) {
            if (j % 3 == 0)
                src[j] = (uint8_t)(lcg_next(&lcg) & 0xFF);
            else
                src[j] = (uint8_t)(j & 0xFF);
        }

        uLongf dst_len = sizeof(dst);
        uint64_t t0 = mach_absolute_time();
        compress2(dst, &dst_len, src, (uLong)sz, Z_DEFAULT_COMPRESSION);
        uint64_t t1 = mach_absolute_time();
        timings[valid++] = t1 - t0;
    }
    return valid;
}

// Cross-correlation: hash_timing (SHA-256 on varying data)
static int collect_hash_timing_cross(uint64_t *timings, int n) {
    uint64_t lcg = mach_absolute_time();
    uint8_t buf[512];
    uint8_t hash[32];

    for (int i = 0; i < n; i++) {
        int sz = 32 + (int)(lcg_next(&lcg) % 481);
        for (int j = 0; j < sz; j++)
            buf[j] = (uint8_t)(lcg_next(&lcg) & 0xFF);

        // Simple iterative hash stand-in: XOR-fold repeatedly
        uint64_t t0 = mach_absolute_time();
        memset(hash, 0, 32);
        for (int j = 0; j < sz; j++)
            hash[j % 32] ^= buf[j];
        for (int r = 0; r < 100; r++)
            for (int j = 0; j < 32; j++)
                hash[j] = (hash[j] * 131 + 1) & 0xFF;
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    return n;
}

// Cross-correlation: amx_timing (CPU workload timing)
static int collect_amx_timing_cross(uint64_t *timings, int n) {
    volatile double acc = 0.0;
    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();
        for (int j = 0; j < 200; j++)
            acc += (double)j * 1.00001;
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    (void)acc;
    return n;
}

int main(void) {
    print_validation_header("compression_timing");

    // === Test 1: Large sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_compression_timing(timings, LARGE_N);
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

    // === Test 3: Stability (10 trials) ===
    printf("=== Test 3: Stability (%d trials x %d samples) ===\n", N_TRIALS, TRIAL_N);
    double min_ents[N_TRIALS];
    uint64_t *trial_buf = malloc(TRIAL_N * sizeof(uint64_t));
    for (int t = 0; t < N_TRIALS; t++) {
        int tv = collect_compression_timing(trial_buf, TRIAL_N);
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
    int my_v = collect_compression_timing(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    const char *cross_names[] = {"hash_timing", "amx_timing"};
    collect_func_t cross_funcs[] = {collect_hash_timing_cross, collect_amx_timing_cross};
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
