// validate_mach_ipc.c — Mach IPC message-passing timing entropy validation
// Mechanism: Pool of 8 Mach ports, complex OOL messages, receiver thread draining
// Compile: cc -O2 -o validate_mach_ipc validate_mach_ipc.c -lm

#include "validate_common.h"
#include <mach/mach.h>

#define PORT_POOL_SIZE 8
#define OOL_SIZE 4096

// Message structures for complex OOL send
typedef struct {
    mach_msg_header_t header;
    mach_msg_body_t body;
    mach_msg_ool_descriptor_t ool;
} ool_send_msg_t;

typedef struct {
    mach_msg_header_t header;
    mach_msg_body_t body;
    mach_msg_ool_descriptor_t ool;
    mach_msg_trailer_t trailer;
} ool_recv_msg_t;

static mach_port_t g_ports[PORT_POOL_SIZE];
static mach_port_t g_send_ports[PORT_POOL_SIZE];
static volatile int g_receiver_running = 1;

static void *receiver_thread(void *arg) {
    (void)arg;
    while (g_receiver_running) {
        // Drain messages from all ports round-robin
        for (int p = 0; p < PORT_POOL_SIZE && g_receiver_running; p++) {
            ool_recv_msg_t recv_msg;
            memset(&recv_msg, 0, sizeof(recv_msg));

            mach_msg_return_t kr = mach_msg(
                &recv_msg.header,
                MACH_RCV_MSG | MACH_RCV_TIMEOUT,
                0,
                sizeof(recv_msg),
                g_ports[p],
                1, // 1ms timeout
                MACH_PORT_NULL
            );

            if (kr == MACH_MSG_SUCCESS) {
                // Deallocate OOL memory if received
                if (recv_msg.ool.address) {
                    vm_deallocate(mach_task_self(),
                                  (vm_address_t)recv_msg.ool.address,
                                  recv_msg.ool.size);
                }
            }
        }
    }
    return NULL;
}

static int collect_mach_ipc(uint64_t *timings, int n) {
    // Create port pool with receive + send rights
    for (int i = 0; i < PORT_POOL_SIZE; i++) {
        kern_return_t kr = mach_port_allocate(mach_task_self(),
                                               MACH_PORT_RIGHT_RECEIVE,
                                               &g_ports[i]);
        if (kr != KERN_SUCCESS) return 0;

        kr = mach_port_insert_right(mach_task_self(), g_ports[i],
                                     g_ports[i], MACH_MSG_TYPE_MAKE_SEND);
        if (kr != KERN_SUCCESS) return 0;
        g_send_ports[i] = g_ports[i];
    }

    // Start receiver thread
    g_receiver_running = 1;
    pthread_t recv_tid;
    pthread_create(&recv_tid, NULL, receiver_thread, NULL);

    // Prepare OOL data
    uint8_t ool_data[OOL_SIZE];
    uint64_t rng = mach_absolute_time();
    for (int i = 0; i < OOL_SIZE; i++) ool_data[i] = (uint8_t)(lcg_next(&rng));

    int valid = 0;
    for (int i = 0; i < n; i++) {
        int port_idx = i % PORT_POOL_SIZE;

        ool_send_msg_t msg;
        memset(&msg, 0, sizeof(msg));
        msg.header.msgh_bits = MACH_MSGH_BITS_COMPLEX |
                               MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, 0);
        msg.header.msgh_size = sizeof(msg);
        msg.header.msgh_remote_port = g_send_ports[port_idx];
        msg.header.msgh_local_port = MACH_PORT_NULL;
        msg.header.msgh_id = i;
        msg.body.msgh_descriptor_count = 1;
        msg.ool.address = ool_data;
        msg.ool.size = OOL_SIZE;
        msg.ool.deallocate = 0;
        msg.ool.copy = MACH_MSG_VIRTUAL_COPY;
        msg.ool.type = MACH_MSG_OOL_DESCRIPTOR;

        uint64_t t0 = mach_absolute_time();
        mach_msg_return_t kr = mach_msg(
            &msg.header,
            MACH_SEND_MSG | MACH_SEND_TIMEOUT,
            sizeof(msg),
            0,
            MACH_PORT_NULL,
            10, // 10ms timeout
            MACH_PORT_NULL
        );
        uint64_t t1 = mach_absolute_time();

        if (kr == MACH_MSG_SUCCESS) {
            timings[valid++] = t1 - t0;
        }
    }

    // Cleanup
    g_receiver_running = 0;
    pthread_join(recv_tid, NULL);

    for (int i = 0; i < PORT_POOL_SIZE; i++) {
        mach_port_deallocate(mach_task_self(), g_send_ports[i]);
        mach_port_mod_refs(mach_task_self(), g_ports[i],
                           MACH_PORT_RIGHT_RECEIVE, -1);
    }

    return valid;
}

// Cross-correlation: thread_lifecycle — kernel allocation
static int collect_thread_lifecycle_simple(uint64_t *timings, int n) {
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

// Cross-correlation: pipe_buffer — kernel IPC
static int collect_pipe_buffer_simple(uint64_t *timings, int n) {
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

int main(void) {
    print_validation_header("mach_ipc");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === TEST 1: 100K sample entropy ===
    printf("=== Test 1: %dK Sample Entropy ===\n", LARGE_N / 1000);
    uint64_t *timings = (uint64_t *)malloc(LARGE_N * sizeof(uint64_t));
    int valid = collect_mach_ipc(timings, LARGE_N);
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
        int tv = collect_mach_ipc(trial_t, TRIAL_N);
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
    int my_v = collect_mach_ipc(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_thread_lifecycle_simple(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "thread_lifecycle", r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" : fabs(r) > 0.1 ? " * weak *" : "");
        free(other);
    }
    {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_pipe_buffer_simple(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        double r = pearson(my_t, other, use);
        printf("  vs %-25s: r=%.4f%s\n", "pipe_buffer", r,
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
