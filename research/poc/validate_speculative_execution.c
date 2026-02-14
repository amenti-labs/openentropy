// validate_speculative_execution.c — Entropy source validation
// Mechanism: Data-dependent branches using LCG (10-40 iterations per batch)
// Cross-correlate: hash_timing, cache_contention
// Compile: cc -O2 -o validate_speculative_execution validate_speculative_execution.c -lm

#include "validate_common.h"

static int collect_speculative_execution(uint64_t *timings, int n) {
    uint64_t lcg = mach_absolute_time();
    volatile int sink = 0;
    int valid = 0;

    for (int i = 0; i < n; i++) {
        // Vary batch size between 10 and 40 iterations
        int batch = 10 + (int)(lcg_next(&lcg) % 31);

        uint64_t t0 = mach_absolute_time();
        int acc = 0;
        for (int j = 0; j < batch; j++) {
            uint64_t v = lcg_next(&lcg);
            // Data-dependent branches the predictor cannot predict
            if (v & 1)
                acc += (int)(v >> 32);
            else
                acc -= (int)(v >> 16);

            if (v & 2)
                acc ^= (int)(v >> 8);
            else
                acc += (int)(v >> 24);

            if (v & 4)
                acc = (acc << 1) | (acc >> 31);
            else
                acc = (acc >> 1) | (acc << 31);

            if ((v >> 3) & 1)
                acc *= 3;
            else
                acc += 7;
        }
        sink = acc;
        uint64_t t1 = mach_absolute_time();
        timings[valid++] = t1 - t0;
    }
    (void)sink;
    return valid;
}

// Cross-correlation: hash_timing (SHA-256 stand-in computation)
static int collect_hash_cross(uint64_t *timings, int n) {
    uint64_t lcg = mach_absolute_time();
    uint8_t buf[256];
    volatile uint8_t hash[32];

    for (int i = 0; i < n; i++) {
        int sz = 32 + (int)(lcg_next(&lcg) % 225);
        for (int j = 0; j < sz; j++)
            buf[j] = (uint8_t)(lcg_next(&lcg) & 0xFF);

        uint64_t t0 = mach_absolute_time();
        // Iterative mixing as hash stand-in
        memset((void *)hash, 0, 32);
        for (int j = 0; j < sz; j++)
            ((volatile uint8_t *)hash)[j % 32] ^= buf[j];
        for (int r = 0; r < 100; r++)
            for (int j = 0; j < 32; j++)
                ((volatile uint8_t *)hash)[j] = (hash[j] * 131 + 1) & 0xFF;
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    return n;
}

// Cross-correlation: cache_contention (memory access patterns)
static int collect_cache_cross(uint64_t *timings, int n) {
    size_t buf_size = 4 * 1024 * 1024;
    volatile uint8_t *buf = (volatile uint8_t *)mmap(NULL, buf_size,
        PROT_READ | PROT_WRITE, MAP_ANON | MAP_PRIVATE, -1, 0);
    if (buf == MAP_FAILED) return 0;

    // Touch pages
    long page_size = sysconf(_SC_PAGESIZE);
    for (size_t off = 0; off < buf_size; off += (size_t)page_size)
        ((volatile uint8_t *)buf)[off] = (uint8_t)(off & 0xFF);

    uint64_t lcg = mach_absolute_time();
    volatile uint8_t sink;

    for (int i = 0; i < n; i++) {
        size_t base = (size_t)(lcg_next(&lcg) % (buf_size - 32768));
        uint64_t t0 = mach_absolute_time();
        for (int j = 0; j < 512; j++) {
            size_t off = base + (size_t)(lcg_next(&lcg) % 32768);
            sink = buf[off];
        }
        uint64_t t1 = mach_absolute_time();
        (void)sink;
        timings[i] = t1 - t0;
    }

    munmap((void *)buf, buf_size);
    return n;
}

int main(void) {
    print_validation_header("speculative_execution");

    // === Test 1: Large sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_speculative_execution(timings, LARGE_N);
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
        int tv = collect_speculative_execution(trial_buf, TRIAL_N);
        Stats ts = compute_stats(trial_buf, tv > 0 ? tv : 1);
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
    int my_v = collect_speculative_execution(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    const char *cross_names[] = {"hash_timing", "cache_contention"};
    collect_func_t cross_funcs[] = {collect_hash_cross, collect_cache_cross};
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
