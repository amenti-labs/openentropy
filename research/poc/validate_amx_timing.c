// validate_amx_timing.c — AMX/Accelerate matrix multiply timing entropy validation
// Mechanism: cblas_sgemm with varying matrix sizes, interleaved volatile memory ops
// Compile: cc -O2 -o validate_amx_timing validate_amx_timing.c -framework Accelerate -lm

#define ACCELERATE_NEW_LAPACK
#include "validate_common.h"
#include <Accelerate/Accelerate.h>

static volatile uint8_t g_scratch[65536];

static int collect_amx_timing(uint64_t *timings, int n) {
    int sizes[] = {16, 32, 48, 64, 96, 128};
    int nsizes = sizeof(sizes) / sizeof(sizes[0]);
    uint64_t rng = mach_absolute_time();

    // Pre-allocate for largest size
    float *a = (float *)malloc(128 * 128 * sizeof(float));
    float *b = (float *)malloc(128 * 128 * sizeof(float));
    float *c = (float *)malloc(128 * 128 * sizeof(float));
    if (!a || !b || !c) { free(a); free(b); free(c); return 0; }

    for (int i = 0; i < 128 * 128; i++) {
        a[i] = (float)(lcg_next(&rng) & 0xFFFF) / 65536.0f;
        b[i] = (float)(lcg_next(&rng) & 0xFFFF) / 65536.0f;
    }

    int valid = 0;
    for (int i = 0; i < n; i++) {
        int sz = sizes[lcg_next(&rng) % nsizes];

        // Interleave volatile memory ops on scratch buffer
        for (int j = 0; j < 16; j++) {
            int off = (int)(lcg_next(&rng) % sizeof(g_scratch));
            g_scratch[off] = (uint8_t)(g_scratch[off] ^ (uint8_t)j);
        }

        uint64_t t0 = mach_absolute_time();
        cblas_sgemm(CblasRowMajor, CblasNoTrans, CblasNoTrans,
                    sz, sz, sz, 1.0f, a, sz, b, sz, 0.0f, c, sz);
        uint64_t t1 = mach_absolute_time();

        timings[valid++] = t1 - t0;
    }

    free(a); free(b); free(c);
    return valid;
}

// Cross-correlation: cache_contention — random memory access timing
static int collect_cache_contention(uint64_t *timings, int n) {
    volatile uint8_t *buf = (volatile uint8_t *)malloc(4 * 1024 * 1024);
    if (!buf) return 0;
    memset((void *)buf, 0xAA, 4 * 1024 * 1024);
    uint64_t rng = mach_absolute_time() ^ 0xDEAD;

    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();
        for (int j = 0; j < 64; j++) {
            int off = (int)(lcg_next(&rng) % (4 * 1024 * 1024));
            (void)buf[off];
        }
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    free((void *)buf);
    return n;
}

// Cross-correlation: compression_timing — computation timing
static int collect_compression_timing(uint64_t *timings, int n) {
    uint64_t rng = mach_absolute_time() ^ 0xBEEF;
    uint8_t buf[4096];

    for (int i = 0; i < n; i++) {
        // Fill buffer with pseudo-random data
        for (int j = 0; j < (int)sizeof(buf); j += 8) {
            uint64_t v = lcg_next(&rng);
            memcpy(buf + j, &v, 8);
        }
        // Simulate computation: repeated XOR-fold
        uint64_t t0 = mach_absolute_time();
        uint64_t acc = 0;
        for (int j = 0; j < (int)sizeof(buf); j++) {
            acc = acc * 31 + buf[j];
        }
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
        (void)acc;
    }
    return n;
}

int main(void) {
    print_validation_header("amx_timing");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === TEST 1: 100K sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = (uint64_t *)malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_amx_timing(timings, LARGE_N);
    Stats s = compute_stats(timings, valid);
    printf("  Samples: %d  Mean=%.1f  StdDev=%.1f\n", valid, s.mean, s.stddev);
    printf("  Shannon=%.3f  H_inf=%.3f\n\n", s.shannon, s.min_entropy);

    // === TEST 2: Autocorrelation (lag 1-5) ===
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

    // === TEST 3: 10 trials stability ===
    printf("=== Test 3: Stability (%d trials x %d samples) ===\n", N_TRIALS, TRIAL_N);
    double min_ents[N_TRIALS];
    uint64_t *trial_t = (uint64_t *)malloc(TRIAL_N * sizeof(uint64_t));
    for (int t = 0; t < N_TRIALS; t++) {
        int tv = collect_amx_timing(trial_t, TRIAL_N);
        Stats ts = compute_stats(trial_t, tv > 0 ? tv : 1);
        min_ents[t] = ts.min_entropy;
        printf("  Trial %2d: H_inf=%.3f  Shannon=%.3f  N=%d\n",
               t + 1, ts.min_entropy, ts.shannon, tv);
    }
    free(trial_t);

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

    // === TEST 4: Cross-correlation ===
    printf("=== Test 4: Cross-correlation ===\n");
    int cc_n = 5000;
    uint64_t *my_t = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
    int my_v = collect_amx_timing(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_cache_contention(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "cache_contention", r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" : fabs(r) > 0.1 ? " * weak *" : "");
        free(other);
    }
    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_compression_timing(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "compression_timing", r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" : fabs(r) > 0.1 ? " * weak *" : "");
        free(other);
    }
    free(my_t);
    printf("\n");

    // === SUMMARY ===
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
