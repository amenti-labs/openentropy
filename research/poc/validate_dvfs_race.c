// validate_dvfs_race.c — DVFS frequency race timing entropy validation
// Mechanism: 2 threads run tight counting loops, measure abs_diff of counts
// Compile: cc -O2 -o validate_dvfs_race validate_dvfs_race.c -lpthread -lm

#include "validate_common.h"
#include <stdatomic.h>

struct race_ctx {
    atomic_uint_fast64_t counter_a;
    atomic_uint_fast64_t counter_b;
    atomic_int ready_a;
    atomic_int ready_b;
    atomic_int stop;
};

static void *racer_a(void *arg) {
    struct race_ctx *ctx = (struct race_ctx *)arg;
    atomic_store(&ctx->ready_a, 1);
    // Spin until both ready
    while (!atomic_load(&ctx->ready_b)) {}

    while (!atomic_load(&ctx->stop)) {
        atomic_fetch_add(&ctx->counter_a, 1);
    }
    return NULL;
}

static void *racer_b(void *arg) {
    struct race_ctx *ctx = (struct race_ctx *)arg;
    atomic_store(&ctx->ready_b, 1);
    // Spin until both ready
    while (!atomic_load(&ctx->ready_a)) {}

    while (!atomic_load(&ctx->stop)) {
        atomic_fetch_add(&ctx->counter_b, 1);
    }
    return NULL;
}

static int collect_dvfs_race(uint64_t *timings, int n) {
    int valid = 0;
    uint64_t prev_diff = 0;

    for (int i = 0; i < n + 1; i++) {
        struct race_ctx ctx;
        atomic_store(&ctx.counter_a, 0);
        atomic_store(&ctx.counter_b, 0);
        atomic_store(&ctx.ready_a, 0);
        atomic_store(&ctx.ready_b, 0);
        atomic_store(&ctx.stop, 0);

        pthread_t ta, tb;
        pthread_create(&ta, NULL, racer_a, &ctx);
        pthread_create(&tb, NULL, racer_b, &ctx);

        // Wait for both threads to be ready
        while (!atomic_load(&ctx.ready_a) || !atomic_load(&ctx.ready_b)) {}

        // Let them race for ~2 microseconds (approx 48 mach ticks at 24MHz timebase)
        uint64_t start = mach_absolute_time();
        while ((mach_absolute_time() - start) < 48) {}

        atomic_store(&ctx.stop, 1);

        pthread_join(ta, NULL);
        pthread_join(tb, NULL);

        uint64_t ca = atomic_load(&ctx.counter_a);
        uint64_t cb = atomic_load(&ctx.counter_b);
        uint64_t diff = (ca > cb) ? (ca - cb) : (cb - ca);

        // XOR adjacent diffs for better entropy extraction
        if (i > 0) {
            timings[valid++] = diff ^ prev_diff;
        }
        prev_diff = diff;

        if (valid >= n) break;
    }
    return valid;
}

// Cross-correlation: cas_contention — multi-thread contention
static int collect_cas_simple(uint64_t *timings, int n) {
    volatile atomic_uint_fast64_t target = 0;

    for (int i = 0; i < n; i++) {
        uint64_t expected = atomic_load(&target);
        uint64_t t0 = mach_absolute_time();
        atomic_compare_exchange_weak(&target, &expected, expected + 1);
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    return n;
}

// Cross-correlation: thread_lifecycle — scheduling
static int collect_thread_simple(uint64_t *timings, int n) {
    for (int i = 0; i < n; i++) {
        pthread_t tid;
        uint64_t t0 = mach_absolute_time();
        pthread_create(&tid, NULL, (void *(*)(void *))mach_absolute_time, NULL);
        pthread_join(tid, NULL);
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    return n;
}

int main(void) {
    print_validation_header("dvfs_race");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === TEST 1: 100K sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = (uint64_t *)malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_dvfs_race(timings, LARGE_N);
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
        int tv = collect_dvfs_race(trial_t, TRIAL_N);
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
    int my_v = collect_dvfs_race(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_cas_simple(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "cas_contention", r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" : fabs(r) > 0.1 ? " * weak *" : "");
        free(other);
    }
    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_thread_simple(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "thread_lifecycle", r,
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
