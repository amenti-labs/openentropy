// validate_cas_contention.c — CAS contention timing entropy validation
// Mechanism: 64 atomic targets (128-byte spaced), 4 threads doing CAS, XOR-combine timings
// Compile: cc -O2 -o validate_cas_contention validate_cas_contention.c -lpthread -lm

#include "validate_common.h"
#include <stdatomic.h>

#define NUM_TARGETS 64
#define TARGET_SPACING 128
#define NUM_CAS_THREADS 4

// Cache-line-isolated atomic targets
static char g_target_buf[NUM_TARGETS * TARGET_SPACING]
    __attribute__((aligned(128)));

static inline atomic_uint_fast64_t *target_at(int idx) {
    return (atomic_uint_fast64_t *)(g_target_buf + idx * TARGET_SPACING);
}

struct cas_thread_ctx {
    int thread_id;
    int samples_per_thread;
    uint64_t *timings;  // output array, size = samples_per_thread
    atomic_int *go;
};

static void *cas_worker(void *arg) {
    struct cas_thread_ctx *ctx = (struct cas_thread_ctx *)arg;
    uint64_t rng = mach_absolute_time() ^ ((uint64_t)ctx->thread_id * 0xDEADBEEF);

    // Wait for go signal
    while (!atomic_load(ctx->go)) {}

    for (int i = 0; i < ctx->samples_per_thread; i++) {
        int tgt = (int)(lcg_next(&rng) % NUM_TARGETS);
        atomic_uint_fast64_t *target = target_at(tgt);

        uint64_t expected = atomic_load(target);
        uint64_t t0 = mach_absolute_time();
        atomic_compare_exchange_weak(target, &expected, expected + 1);
        uint64_t t1 = mach_absolute_time();

        ctx->timings[i] = t1 - t0;
    }
    return NULL;
}

static int collect_cas_contention(uint64_t *timings, int n) {
    // Initialize targets
    memset(g_target_buf, 0, sizeof(g_target_buf));
    for (int i = 0; i < NUM_TARGETS; i++) {
        atomic_store(target_at(i), 0);
    }

    int samples_per_thread = n / NUM_CAS_THREADS;
    if (samples_per_thread < 1) samples_per_thread = 1;

    // Allocate per-thread timing arrays
    uint64_t *thread_timings[NUM_CAS_THREADS];
    for (int i = 0; i < NUM_CAS_THREADS; i++) {
        thread_timings[i] = (uint64_t *)malloc(samples_per_thread * sizeof(uint64_t));
        if (!thread_timings[i]) {
            for (int j = 0; j < i; j++) free(thread_timings[j]);
            return 0;
        }
    }

    atomic_int go = 0;
    struct cas_thread_ctx ctxs[NUM_CAS_THREADS];
    pthread_t tids[NUM_CAS_THREADS];

    for (int i = 0; i < NUM_CAS_THREADS; i++) {
        ctxs[i].thread_id = i;
        ctxs[i].samples_per_thread = samples_per_thread;
        ctxs[i].timings = thread_timings[i];
        ctxs[i].go = &go;
        pthread_create(&tids[i], NULL, cas_worker, &ctxs[i]);
    }

    // Signal all threads to start
    atomic_store(&go, 1);

    for (int i = 0; i < NUM_CAS_THREADS; i++) {
        pthread_join(tids[i], NULL);
    }

    // XOR-combine timings from all threads
    int valid = 0;
    for (int s = 0; s < samples_per_thread && valid < n; s++) {
        uint64_t combined = 0;
        for (int t = 0; t < NUM_CAS_THREADS; t++) {
            combined ^= thread_timings[t][s];
        }
        timings[valid++] = combined;
    }

    for (int i = 0; i < NUM_CAS_THREADS; i++) {
        free(thread_timings[i]);
    }
    return valid;
}

// Cross-correlation: dvfs_race — multi-thread contention
static int collect_dvfs_simple(uint64_t *timings, int n) {
    for (int i = 0; i < n; i++) {
        // Two threads doing atomic increments briefly
        atomic_uint_fast64_t counter = 0;
        atomic_int stop = 0;

        // Single-threaded approximation for cross-correlation
        uint64_t t0 = mach_absolute_time();
        for (int j = 0; j < 100; j++) {
            atomic_fetch_add(&counter, 1);
        }
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
        (void)stop;
    }
    return n;
}

// Cross-correlation: cache_contention — cache operations
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

int main(void) {
    print_validation_header("cas_contention");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === TEST 1: 100K sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = (uint64_t *)malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_cas_contention(timings, LARGE_N);
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
        int tv = collect_cas_contention(trial_t, TRIAL_N);
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
    int my_v = collect_cas_contention(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_dvfs_simple(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "dvfs_race", r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" : fabs(r) > 0.1 ? " * weak *" : "");
        free(other);
    }
    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_cache_contention(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "cache_contention", r,
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
