// validate_kqueue_events.c — kqueue event notification timing entropy validation
// Mechanism: kqueue with 8 timers, 4 socket pairs, 4 file watchers; background poking
// Compile: cc -O2 -o validate_kqueue_events validate_kqueue_events.c -lm

#include "validate_common.h"
#include <sys/event.h>
#include <sys/socket.h>
#include <sys/stat.h>

#define NUM_TIMERS 8
#define NUM_SOCKETS 4
#define NUM_FILES 4

struct kqueue_ctx {
    int kq;
    int sock_pairs[NUM_SOCKETS][2];
    char tmpfiles[NUM_FILES][128];
    int tmpfds[NUM_FILES];
    volatile int running;
};

static void *background_poker(void *arg) {
    struct kqueue_ctx *ctx = (struct kqueue_ctx *)arg;
    uint64_t rng = mach_absolute_time() ^ 0xBEEF;
    uint8_t poke = 0x42;

    while (ctx->running) {
        // Poke a random socket
        int si = (int)(lcg_next(&rng) % NUM_SOCKETS);
        write(ctx->sock_pairs[si][0], &poke, 1);

        // Touch a random file
        int fi = (int)(lcg_next(&rng) % NUM_FILES);
        if (ctx->tmpfds[fi] >= 0) {
            lseek(ctx->tmpfds[fi], 0, SEEK_SET);
            write(ctx->tmpfds[fi], &poke, 1);
        }

        // Small random delay
        usleep(100 + (int)(lcg_next(&rng) % 500));
    }
    return NULL;
}

static int setup_kqueue_ctx(struct kqueue_ctx *ctx) {
    ctx->kq = kqueue();
    if (ctx->kq < 0) return -1;

    struct kevent evs[NUM_TIMERS + NUM_SOCKETS * 2 + NUM_FILES];
    int nev = 0;

    // Register 8 timers with 1-10ms intervals
    for (int i = 0; i < NUM_TIMERS; i++) {
        int ms = 1 + (i % 10);
        EV_SET(&evs[nev++], 100 + i, EVFILT_TIMER, EV_ADD, 0, ms, NULL);
    }

    // Create 4 socket pairs with EVFILT_READ on read end
    for (int i = 0; i < NUM_SOCKETS; i++) {
        if (socketpair(AF_UNIX, SOCK_STREAM, 0, ctx->sock_pairs[i]) != 0) {
            ctx->sock_pairs[i][0] = ctx->sock_pairs[i][1] = -1;
            continue;
        }
        // Non-blocking
        fcntl(ctx->sock_pairs[i][1], F_SETFL,
              fcntl(ctx->sock_pairs[i][1], F_GETFL) | O_NONBLOCK);
        EV_SET(&evs[nev++], ctx->sock_pairs[i][1], EVFILT_READ, EV_ADD, 0, 0, NULL);
    }

    // Create 4 temp files with EVFILT_VNODE
    for (int i = 0; i < NUM_FILES; i++) {
        snprintf(ctx->tmpfiles[i], sizeof(ctx->tmpfiles[i]),
                 "/tmp/openentropy_kq_validate_%d_%d", getpid(), i);
        ctx->tmpfds[i] = open(ctx->tmpfiles[i], O_CREAT | O_RDWR, 0600);
        if (ctx->tmpfds[i] >= 0) {
            EV_SET(&evs[nev++], ctx->tmpfds[i], EVFILT_VNODE,
                   EV_ADD | EV_CLEAR,
                   NOTE_WRITE | NOTE_ATTRIB,
                   0, NULL);
        }
    }

    kevent(ctx->kq, evs, nev, NULL, 0, NULL);
    ctx->running = 1;
    return 0;
}

static void cleanup_kqueue_ctx(struct kqueue_ctx *ctx) {
    ctx->running = 0;
    close(ctx->kq);
    for (int i = 0; i < NUM_SOCKETS; i++) {
        if (ctx->sock_pairs[i][0] >= 0) close(ctx->sock_pairs[i][0]);
        if (ctx->sock_pairs[i][1] >= 0) close(ctx->sock_pairs[i][1]);
    }
    for (int i = 0; i < NUM_FILES; i++) {
        if (ctx->tmpfds[i] >= 0) close(ctx->tmpfds[i]);
        unlink(ctx->tmpfiles[i]);
    }
}

static int collect_kqueue_events(uint64_t *timings, int n) {
    struct kqueue_ctx ctx;
    if (setup_kqueue_ctx(&ctx) != 0) return 0;

    // Start background poker thread
    pthread_t poker_tid;
    pthread_create(&poker_tid, NULL, background_poker, &ctx);

    // Small warmup delay
    usleep(5000);

    struct kevent out_evs[32];
    struct timespec timeout = {0, 1000000}; // 1ms timeout
    uint8_t drain_buf[256];

    int valid = 0;
    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();
        int nready = kevent(ctx.kq, NULL, 0, out_evs, 32, &timeout);
        uint64_t t1 = mach_absolute_time();

        timings[valid++] = t1 - t0;

        // Drain any socket data to prevent buffer fill
        if (nready > 0) {
            for (int j = 0; j < nready; j++) {
                if (out_evs[j].filter == EVFILT_READ) {
                    read((int)out_evs[j].ident, drain_buf, sizeof(drain_buf));
                }
            }
        }
    }

    ctx.running = 0;
    pthread_join(poker_tid, NULL);
    cleanup_kqueue_ctx(&ctx);
    return valid;
}

// Cross-correlation: pipe_buffer — kernel event notification
static int collect_pipe_simple(uint64_t *timings, int n) {
    int pfd[2];
    if (pipe(pfd) != 0) return 0;
    uint8_t buf[256];
    memset(buf, 0x42, sizeof(buf));

    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();
        write(pfd[1], buf, sizeof(buf));
        read(pfd[0], buf, sizeof(buf));
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }
    close(pfd[0]); close(pfd[1]);
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
    print_validation_header("kqueue_events");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === TEST 1: 100K sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = (uint64_t *)malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_kqueue_events(timings, LARGE_N);
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
        int tv = collect_kqueue_events(trial_t, TRIAL_N);
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
    int my_v = collect_kqueue_events(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_pipe_simple(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "pipe_buffer", r,
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
