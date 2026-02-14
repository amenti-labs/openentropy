// validate_vm_page_timing.c — Entropy source validation
// Mechanism: mmap(MAP_ANON), write_volatile, read_volatile, munmap cycle timing
// Cross-correlate: page_fault_timing, tlb_shootdown
// Compile: cc -O2 -o validate_vm_page_timing validate_vm_page_timing.c -lm

#include "validate_common.h"

static int collect_vm_page_timing(uint64_t *timings, int n) {
    long page_size = sysconf(_SC_PAGESIZE);
    int valid = 0;

    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();

        volatile uint8_t *p = (volatile uint8_t *)mmap(NULL, (size_t)page_size,
            PROT_READ | PROT_WRITE, MAP_ANON | MAP_PRIVATE, -1, 0);
        if (p == MAP_FAILED) continue;

        // Write volatile to force page materialization
        *p = 0x42;
        p[page_size / 2] = 0x43;

        // Read volatile
        volatile uint8_t v = *p;
        v = p[page_size / 2];
        (void)v;

        munmap((void *)p, (size_t)page_size);

        uint64_t t1 = mach_absolute_time();
        timings[valid++] = t1 - t0;
    }
    return valid;
}

// Cross-correlation: page_fault_timing (mmap + touch pages)
static int collect_page_fault_cross(uint64_t *timings, int n) {
    long page_size = sysconf(_SC_PAGESIZE);
    int valid = 0;

    for (int i = 0; i < n; i++) {
        volatile uint8_t *p = (volatile uint8_t *)mmap(NULL, (size_t)(page_size * 2),
            PROT_READ | PROT_WRITE, MAP_ANON | MAP_PRIVATE, -1, 0);
        if (p == MAP_FAILED) continue;

        uint64_t t0 = mach_absolute_time();
        // Touch two pages to trigger minor faults
        *p = 0x01;
        p[page_size] = 0x02;
        uint64_t t1 = mach_absolute_time();

        munmap((void *)p, (size_t)(page_size * 2));
        timings[valid++] = t1 - t0;
    }
    return valid;
}

// Cross-correlation: tlb_shootdown (mprotect cycle)
static int collect_tlb_cross(uint64_t *timings, int n) {
    long page_size = sysconf(_SC_PAGESIZE);
    int valid = 0;

    for (int i = 0; i < n; i++) {
        volatile uint8_t *p = (volatile uint8_t *)mmap(NULL, (size_t)page_size,
            PROT_READ | PROT_WRITE, MAP_ANON | MAP_PRIVATE, -1, 0);
        if (p == MAP_FAILED) continue;
        *p = 0x01; // Populate

        uint64_t t0 = mach_absolute_time();
        mprotect((void *)p, (size_t)page_size, PROT_READ);
        mprotect((void *)p, (size_t)page_size, PROT_READ | PROT_WRITE);
        uint64_t t1 = mach_absolute_time();

        munmap((void *)p, (size_t)page_size);
        timings[valid++] = t1 - t0;
    }
    return valid;
}

int main(void) {
    print_validation_header("vm_page_timing");

    // === Test 1: Large sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_vm_page_timing(timings, LARGE_N);
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
        int tv = collect_vm_page_timing(trial_buf, TRIAL_N);
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
    int my_v = collect_vm_page_timing(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    const char *cross_names[] = {"page_fault_timing", "tlb_shootdown"};
    collect_func_t cross_funcs[] = {collect_page_fault_cross, collect_tlb_cross};
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
