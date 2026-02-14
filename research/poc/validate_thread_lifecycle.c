// validate_thread_lifecycle.c — Thread create/join timing entropy validation
// Mechanism: Create pthread, run small workload (0-100 iterations), join, measure total time
// Compile: cc -O2 -o validate_thread_lifecycle validate_thread_lifecycle.c -lpthread -lm

#include "validate_common.h"

struct thread_work {
    int iterations;
    volatile uint64_t result;
};

static void *thread_worker(void *arg) {
    struct thread_work *w = (struct thread_work *)arg;
    volatile uint64_t acc = 0;
    for (int i = 0; i < w->iterations; i++) {
        acc += (uint64_t)i * 7 + 13;
    }
    w->result = acc;
    return NULL;
}

static int collect_thread_lifecycle(uint64_t *timings, int n) {
    uint64_t rng = mach_absolute_time();
    int valid = 0;

    for (int i = 0; i < n; i++) {
        struct thread_work work;
        work.iterations = (int)(lcg_next(&rng) % 101); // 0-100
        work.result = 0;

        pthread_t tid;
        uint64_t t0 = mach_absolute_time();
        if (pthread_create(&tid, NULL, thread_worker, &work) != 0) continue;
        pthread_join(tid, NULL);
        uint64_t t1 = mach_absolute_time();

        timings[valid++] = t1 - t0;
    }
    return valid;
}

// Cross-correlation: dispatch_queue — thread scheduling
static int collect_dispatch_queue(uint64_t *timings, int n) {
    uint64_t rng = mach_absolute_time() ^ 0xCAFE;
    for (int i = 0; i < n; i++) {
        volatile uint64_t acc = 0;
        int iters = (int)(lcg_next(&rng) % 50);

        uint64_t t0 = mach_absolute_time();
        // Simulate dispatch by creating a thread with minimal work
        pthread_t tid;
        struct thread_work work = { .iterations = iters, .result = 0 };
        if (pthread_create(&tid, NULL, thread_worker, &work) == 0) {
            pthread_join(tid, NULL);
            acc = work.result;
        }
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
        (void)acc;
    }
    return n;
}

// Cross-correlation: mach_ipc — kernel port allocation
static int collect_mach_ipc_simple(uint64_t *timings, int n) {
    for (int i = 0; i < n; i++) {
        mach_port_t port;
        uint64_t t0 = mach_absolute_time();
        mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &port);
        mach_port_deallocate(mach_task_self(), port);
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    return n;
}

int main(void) {
    print_validation_header("thread_lifecycle");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === TEST 1: 100K sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = (uint64_t *)malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_thread_lifecycle(timings, LARGE_N);
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
        int tv = collect_thread_lifecycle(trial_t, TRIAL_N);
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
    int my_v = collect_thread_lifecycle(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_dispatch_queue(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "dispatch_queue", r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" : fabs(r) > 0.1 ? " * weak *" : "");
        free(other);
    }
    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_mach_ipc_simple(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "mach_ipc", r,
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
