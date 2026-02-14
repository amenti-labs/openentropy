// validate_dispatch_queue.c — Entropy source validation
// Mechanism: 4 worker pthreads with pipe-based IPC, measure scheduling latency
// Cross-correlate: thread_lifecycle, kqueue_events
// Compile: cc -O2 -o validate_dispatch_queue validate_dispatch_queue.c -lpthread -lm

#include "validate_common.h"

#define NUM_WORKERS 4

typedef struct {
    int pipe_to_worker[2];   // main -> worker
    int pipe_from_worker[2]; // worker -> main
    volatile int running;
} WorkerCtx;

static void *worker_thread(void *arg) {
    WorkerCtx *ctx = (WorkerCtx *)arg;
    uint64_t ts;
    while (ctx->running) {
        ssize_t r = read(ctx->pipe_to_worker[0], &ts, sizeof(ts));
        if (r != sizeof(ts)) break;
        uint64_t now = mach_absolute_time();
        uint64_t latency = now - ts;
        write(ctx->pipe_from_worker[1], &latency, sizeof(latency));
    }
    return NULL;
}

static WorkerCtx g_workers[NUM_WORKERS];
static pthread_t g_threads[NUM_WORKERS];
static int g_workers_started = 0;

static void start_workers(void) {
    if (g_workers_started) return;
    for (int i = 0; i < NUM_WORKERS; i++) {
        pipe(g_workers[i].pipe_to_worker);
        pipe(g_workers[i].pipe_from_worker);
        g_workers[i].running = 1;
        pthread_create(&g_threads[i], NULL, worker_thread, &g_workers[i]);
    }
    g_workers_started = 1;
    usleep(10000); // Let workers settle
}

static void stop_workers(void) {
    if (!g_workers_started) return;
    for (int i = 0; i < NUM_WORKERS; i++) {
        g_workers[i].running = 0;
        close(g_workers[i].pipe_to_worker[1]);
        pthread_join(g_threads[i], NULL);
        close(g_workers[i].pipe_to_worker[0]);
        close(g_workers[i].pipe_from_worker[0]);
        close(g_workers[i].pipe_from_worker[1]);
    }
    g_workers_started = 0;
}

static int collect_dispatch_queue(uint64_t *timings, int n) {
    start_workers();
    int valid = 0;

    for (int i = 0; i < n; i++) {
        int w = i % NUM_WORKERS;
        uint64_t ts = mach_absolute_time();
        ssize_t wr = write(g_workers[w].pipe_to_worker[1], &ts, sizeof(ts));
        if (wr != sizeof(ts)) continue;

        uint64_t latency;
        ssize_t rd = read(g_workers[w].pipe_from_worker[0], &latency, sizeof(latency));
        if (rd == sizeof(latency)) {
            timings[valid++] = latency;
        }
    }
    return valid;
}

// Cross-correlation: thread_lifecycle (thread create/join timing)
static int collect_thread_lifecycle_cross(uint64_t *timings, int n) {
    for (int i = 0; i < n; i++) {
        pthread_t th;
        uint64_t t0 = mach_absolute_time();
        pthread_create(&th, NULL, (void *(*)(void *))pthread_exit, NULL);
        pthread_join(th, NULL);
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    return n;
}

// Cross-correlation: kqueue_events (pipe event notification)
static int collect_kqueue_cross(uint64_t *timings, int n) {
    for (int i = 0; i < n; i++) {
        int pfd[2];
        pipe(pfd);
        uint8_t byte = 0x42;
        uint64_t t0 = mach_absolute_time();
        write(pfd[1], &byte, 1);
        read(pfd[0], &byte, 1);
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
        close(pfd[0]);
        close(pfd[1]);
    }
    return n;
}

int main(void) {
    print_validation_header("dispatch_queue");

    // === Test 1: Large sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_dispatch_queue(timings, LARGE_N);
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
        int tv = collect_dispatch_queue(trial_buf, TRIAL_N);
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
    int my_v = collect_dispatch_queue(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    stop_workers(); // Stop workers before cross-correlation tests

    const char *cross_names[] = {"thread_lifecycle", "kqueue_events"};
    collect_func_t cross_funcs[] = {collect_thread_lifecycle_cross, collect_kqueue_cross};
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
