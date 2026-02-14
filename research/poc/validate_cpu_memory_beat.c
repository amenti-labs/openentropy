// validate_cpu_memory_beat.c — CPU/Memory cross-domain beat timing entropy validation
// Mechanism: Allocate 16MB buffer, touch pages. Alternate: 50 LCG iterations (CPU),
//            then random read_volatile from buffer (memory). Record both domain timings,
//            interleave into timings array.
// Cross-correlate with: cpu_io_beat (cross-domain), dram_row_buffer (memory access)
// Compile: cc -O2 -o validate_cpu_memory_beat validate_cpu_memory_beat.c -lm

#include "validate_common.h"

#define MEM_BUF_SIZE (16 * 1024 * 1024)

static int collect_cpu_memory_beat(uint64_t *timings, int n) {
    volatile uint8_t *buf = (volatile uint8_t *)malloc(MEM_BUF_SIZE);
    if (!buf) return 0;

    // Touch all pages to fault them in
    for (size_t i = 0; i < MEM_BUF_SIZE; i += 4096) {
        buf[i] = (uint8_t)(i & 0xFF);
    }

    uint64_t rng = mach_absolute_time();
    int valid = 0;
    int iterations = n / 2;

    for (int i = 0; i < iterations && valid + 1 < n; i++) {
        // CPU domain: 50 LCG iterations
        uint64_t t0 = mach_absolute_time();
        for (int j = 0; j < 50; j++) {
            lcg_next(&rng);
        }
        uint64_t t1 = mach_absolute_time();
        timings[valid++] = t1 - t0;

        // Memory domain: random volatile read from 16MB buffer
        size_t off = (size_t)(lcg_next(&rng) % MEM_BUF_SIZE);
        uint64_t t2 = mach_absolute_time();
        (void)buf[off];
        uint64_t t3 = mach_absolute_time();
        timings[valid++] = t3 - t2;
    }

    free((void *)buf);
    return valid;
}

// Cross-correlation: cpu_io_beat — alternate CPU + disk I/O
static int collect_cpu_io_beat(uint64_t *timings, int n) {
    char tmppath[] = "/tmp/oe_xc_io_XXXXXX";
    int fd = mkstemp(tmppath);
    if (fd < 0) return 0;
    unlink(tmppath);

    FILE *fp = fdopen(fd, "w");
    if (!fp) { close(fd); return 0; }

    uint64_t rng = mach_absolute_time() ^ 0xFACE;
    uint8_t wbuf[64];
    int valid = 0;
    int iterations = n / 2;

    for (int i = 0; i < iterations && valid + 1 < n; i++) {
        // CPU
        uint64_t t0 = mach_absolute_time();
        for (int j = 0; j < 50; j++) lcg_next(&rng);
        uint64_t t1 = mach_absolute_time();
        timings[valid++] = t1 - t0;

        // IO
        for (int j = 0; j < 64; j++) wbuf[j] = (uint8_t)(lcg_next(&rng) & 0xFF);
        uint64_t t2 = mach_absolute_time();
        fwrite(wbuf, 1, 64, fp);
        if ((i & 15) == 0) fflush(fp);
        uint64_t t3 = mach_absolute_time();
        timings[valid++] = t3 - t2;
    }

    fclose(fp);
    return valid;
}

// Cross-correlation: dram_row_buffer — sequential vs random memory access timing
static int collect_dram_row_buffer(uint64_t *timings, int n) {
    size_t bufsz = 16 * 1024 * 1024;
    volatile uint8_t *buf = (volatile uint8_t *)malloc(bufsz);
    if (!buf) return 0;
    memset((void *)buf, 0x55, bufsz);

    uint64_t rng = mach_absolute_time() ^ 0xD1CE;

    for (int i = 0; i < n; i++) {
        // Random stride access to trigger row buffer misses
        size_t off = (size_t)(lcg_next(&rng) % bufsz);
        uint64_t t0 = mach_absolute_time();
        (void)buf[off];
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }

    free((void *)buf);
    return n;
}

int main(void) {
    print_validation_header("cpu_memory_beat");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === TEST 1: 100K sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = (uint64_t *)malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_cpu_memory_beat(timings, LARGE_N);
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
        int tv = collect_cpu_memory_beat(trial_t, TRIAL_N);
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
    int my_v = collect_cpu_memory_beat(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_cpu_io_beat(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "cpu_io_beat", r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" : fabs(r) > 0.1 ? " * weak *" : "");
        free(other);
    }
    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_dram_row_buffer(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "dram_row_buffer", r,
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
