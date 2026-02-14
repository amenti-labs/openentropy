// validate_cache_contention.c — Entropy source validation
// Mechanism: 8MB buffer, alternate sequential/random/strided-64 access patterns (512 reads each)
// Cross-correlate: dram_row_buffer, speculative_execution
// Compile: cc -O2 -o validate_cache_contention validate_cache_contention.c -lm

#include "validate_common.h"

#define CACHE_BUF_SIZE (8 * 1024 * 1024)

static volatile uint8_t *g_cache_buf = NULL;

static void ensure_cache_buf(void) {
    if (g_cache_buf) return;
    g_cache_buf = (volatile uint8_t *)mmap(NULL, CACHE_BUF_SIZE,
        PROT_READ | PROT_WRITE, MAP_ANON | MAP_PRIVATE, -1, 0);
    if (g_cache_buf == MAP_FAILED) {
        g_cache_buf = NULL;
        return;
    }
    // Touch every page
    long page_size = sysconf(_SC_PAGESIZE);
    for (size_t off = 0; off < CACHE_BUF_SIZE; off += (size_t)page_size)
        ((volatile uint8_t *)g_cache_buf)[off] = (uint8_t)(off & 0xFF);
}

static int collect_cache_contention(uint64_t *timings, int n) {
    ensure_cache_buf();
    if (!g_cache_buf) return 0;

    uint64_t lcg = mach_absolute_time();
    int valid = 0;
    volatile uint8_t sink;

    for (int i = 0; i < n; i++) {
        int pattern = i % 3; // 0=sequential, 1=random, 2=strided-64
        size_t base = (size_t)(lcg_next(&lcg) % (CACHE_BUF_SIZE - 65536));

        uint64_t t0 = mach_absolute_time();
        switch (pattern) {
        case 0: // Sequential: 512 consecutive reads
            for (int j = 0; j < 512; j++)
                sink = g_cache_buf[base + j];
            break;
        case 1: // Random: 512 random reads within 64K window
            for (int j = 0; j < 512; j++) {
                size_t off = base + (size_t)(lcg_next(&lcg) % 65536);
                sink = g_cache_buf[off];
            }
            break;
        case 2: // Strided-64: 512 reads at stride 64 (cache-line sized)
            for (int j = 0; j < 512; j++)
                sink = g_cache_buf[base + (size_t)j * 64];
            break;
        }
        uint64_t t1 = mach_absolute_time();
        (void)sink;
        timings[valid++] = t1 - t0;
    }
    return valid;
}

// Cross-correlation: dram_row_buffer (distant random reads)
static int collect_dram_cross(uint64_t *timings, int n) {
    ensure_cache_buf();
    if (!g_cache_buf) return 0;

    uint64_t lcg = mach_absolute_time();
    volatile uint8_t sink;

    for (int i = 0; i < n; i++) {
        size_t off1 = (size_t)(lcg_next(&lcg) % (CACHE_BUF_SIZE / 2));
        size_t off2 = (CACHE_BUF_SIZE / 2) + (size_t)(lcg_next(&lcg) % (CACHE_BUF_SIZE / 2));

        uint64_t t0 = mach_absolute_time();
        sink = g_cache_buf[off1];
        sink = g_cache_buf[off2];
        uint64_t t1 = mach_absolute_time();
        (void)sink;
        timings[i] = t1 - t0;
    }
    return n;
}

// Cross-correlation: speculative_execution (branch-heavy CPU work)
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
    print_validation_header("cache_contention");

    // === Test 1: Large sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_cache_contention(timings, LARGE_N);
    if (valid < 100) {
        printf("  FAIL: Only got %d samples\n", valid);
        free(timings);
        return 1;
    }
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
        int tv = collect_cache_contention(trial_buf, TRIAL_N);
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
    int my_v = collect_cache_contention(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    const char *cross_names[] = {"dram_row_buffer", "speculative_execution"};
    collect_func_t cross_funcs[] = {collect_dram_cross, collect_speculative_cross};
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

    // Cleanup
    if (g_cache_buf) munmap((void *)g_cache_buf, CACHE_BUF_SIZE);

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
