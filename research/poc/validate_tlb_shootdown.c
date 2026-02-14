// validate_tlb_shootdown.c — TLB shootdown timing entropy validation
// Mechanism: mmap 256-page region, mprotect random page ranges, measure timing variance
// Compile: cc -O2 -o validate_tlb_shootdown validate_tlb_shootdown.c -lm

#include "validate_common.h"

#define TLB_PAGES 256
#define TLB_REGION_SIZE (TLB_PAGES * 4096)

static int collect_tlb_shootdown(uint64_t *timings, int n) {
    void *region = mmap(NULL, TLB_REGION_SIZE, PROT_READ | PROT_WRITE,
                        MAP_ANON | MAP_PRIVATE, -1, 0);
    if (region == MAP_FAILED) return 0;

    // Touch all pages to populate TLB
    volatile uint8_t *p = (volatile uint8_t *)region;
    for (int i = 0; i < TLB_PAGES; i++) {
        p[i * 4096] = (uint8_t)i;
    }

    uint64_t rng = mach_absolute_time();
    int valid = 0;
    uint64_t prev_delta = 0;

    for (int i = 0; i < n + 1; i++) {
        // Random page count 8-128 and random offset
        int page_count = 8 + (int)(lcg_next(&rng) % 121); // 8-128
        int max_off = TLB_PAGES - page_count;
        if (max_off < 1) max_off = 1;
        int offset = (int)(lcg_next(&rng) % max_off);

        void *target = (uint8_t *)region + offset * 4096;
        size_t len = (size_t)page_count * 4096;

        uint64_t t0 = mach_absolute_time();
        mprotect(target, len, PROT_READ);
        mprotect(target, len, PROT_READ | PROT_WRITE);
        uint64_t t1 = mach_absolute_time();

        uint64_t delta = t1 - t0;

        // Use delta-of-deltas (variance extraction) for entropy
        if (i > 0) {
            uint64_t dd = (delta > prev_delta) ? (delta - prev_delta) : (prev_delta - delta);
            timings[valid++] = dd;
        }
        prev_delta = delta;

        if (valid >= n) break;
    }

    munmap(region, TLB_REGION_SIZE);
    return valid;
}

// Cross-correlation: page_fault_timing — VM operations
static int collect_page_fault_timing(uint64_t *timings, int n) {
    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();
        void *p = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                        MAP_ANON | MAP_PRIVATE, -1, 0);
        if (p != MAP_FAILED) {
            *(volatile uint8_t *)p = 0x42; // Trigger page fault
            munmap(p, 4096);
        }
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    return n;
}

// Cross-correlation: vm_page_timing — mmap/munmap
static int collect_vm_page_timing(uint64_t *timings, int n) {
    uint64_t rng = mach_absolute_time() ^ 0xF00D;
    for (int i = 0; i < n; i++) {
        int pages = 1 + (int)(lcg_next(&rng) % 16);
        size_t sz = (size_t)pages * 4096;

        uint64_t t0 = mach_absolute_time();
        void *p = mmap(NULL, sz, PROT_READ | PROT_WRITE,
                        MAP_ANON | MAP_PRIVATE, -1, 0);
        if (p != MAP_FAILED) {
            *(volatile uint8_t *)p = 0x42;
            munmap(p, sz);
        }
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    return n;
}

int main(void) {
    print_validation_header("tlb_shootdown");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === TEST 1: 100K sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = (uint64_t *)malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_tlb_shootdown(timings, LARGE_N);
    if (valid < 100) {
        printf("  FAIL: Only got %d samples (need >= 100)\n", valid);
        free(timings);
        return 1;
    }
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
        int tv = collect_tlb_shootdown(trial_t, TRIAL_N);
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
    int my_v = collect_tlb_shootdown(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_page_fault_timing(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "page_fault_timing", r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" : fabs(r) > 0.1 ? " * weak *" : "");
        free(other);
    }
    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_vm_page_timing(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "vm_page_timing", r,
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
