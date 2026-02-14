// validate_dyld_timing.c — Entropy source validation
// Mechanism: dlopen/dlclose system libraries in a cycle, measure timing
// Cross-correlate: spotlight_timing, compression_timing
// Compile: cc -O2 -o validate_dyld_timing validate_dyld_timing.c -lm -ldl

#include "validate_common.h"
#include <dlfcn.h>

static const char *g_libs[] = {
    "/usr/lib/libz.dylib",
    "/usr/lib/libc++.dylib",
    "/usr/lib/libobjc.dylib",
    "/usr/lib/libSystem.B.dylib",
};
static const int g_nlibs = 4;

static int collect_dyld_timing(uint64_t *timings, int n) {
    int valid = 0;
    for (int i = 0; i < n; i++) {
        const char *lib = g_libs[i % g_nlibs];
        uint64_t t0 = mach_absolute_time();
        void *h = dlopen(lib, RTLD_LAZY | RTLD_NOLOAD);
        if (!h) h = dlopen(lib, RTLD_LAZY);
        if (h) dlclose(h);
        uint64_t t1 = mach_absolute_time();
        timings[valid++] = t1 - t0;
    }
    return valid;
}

// Cross-correlation: spotlight_timing (process/filesystem)
static int collect_spotlight_cross(uint64_t *timings, int n) {
    // Lightweight filesystem stat as stand-in for spotlight
    const char *files[] = {"/usr/bin/true", "/usr/bin/false", "/usr/bin/env", "/usr/bin/id"};
    for (int i = 0; i < n; i++) {
        const char *f = files[i % 4];
        uint64_t t0 = mach_absolute_time();
        int fd = open(f, O_RDONLY);
        if (fd >= 0) {
            char buf[64];
            read(fd, buf, sizeof(buf));
            close(fd);
        }
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    return n;
}

// Cross-correlation: compression_timing (computation)
static int collect_compression_cross(uint64_t *timings, int n) {
    uint64_t lcg = mach_absolute_time();
    volatile uint32_t sink;
    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();
        uint32_t cs = 0;
        int sz = 128 + (int)(lcg_next(&lcg) % 385);
        for (int j = 0; j < sz; j++)
            cs = (cs << 1) ^ ((lcg_next(&lcg) >> 8) & 0xFF);
        for (int r = 0; r < 50; r++)
            cs = cs * 2654435761U + 1;
        sink = cs;
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    (void)sink;
    return n;
}

int main(void) {
    print_validation_header("dyld_timing");

    // === Test 1: Large sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_dyld_timing(timings, LARGE_N);
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
        int tv = collect_dyld_timing(trial_buf, TRIAL_N);
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
    int my_v = collect_dyld_timing(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    const char *cross_names[] = {"spotlight_timing", "compression_timing"};
    collect_func_t cross_funcs[] = {collect_spotlight_cross, collect_compression_cross};
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
