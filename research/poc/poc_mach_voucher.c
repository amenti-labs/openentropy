/*
 * PoC 4: Mach Voucher and Thread QoS Scheduling Entropy
 *
 * Physics: macOS uses Mach vouchers for QoS (Quality of Service) propagation.
 * The kernel's CLUTCH scheduler makes per-thread scheduling decisions based on:
 * - Thread QoS tier (user-interactive, user-initiated, utility, background)
 * - Process importance (foreground vs background)
 * - Thermal pressure (from CLPC - Closed Loop Performance Controller)
 * - CPU time decay (threads that used more CPU get deprioritized)
 *
 * By rapidly changing a thread's QoS tier and measuring the scheduling
 * latency, we observe the CLUTCH scheduler's internal state — which depends
 * on ALL threads across ALL processes. This is a fundamentally different
 * entropy domain from our existing thread_lifecycle source (which measures
 * create/join, not QoS scheduling decisions).
 *
 * Additional: task_info() returns micro-variations in scheduling statistics
 * that change with each context switch.
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <pthread.h>
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <mach/thread_policy.h>
#include <math.h>
#include <sys/resource.h>

#define NUM_SAMPLES 10000

// QoS classes we can set via pthread
#define QOS_BACKGROUND      0x09      // QOS_CLASS_BACKGROUND
#define QOS_UTILITY         0x11      // QOS_CLASS_UTILITY
#define QOS_USER_INITIATED  0x19      // QOS_CLASS_USER_INITIATED
#define QOS_USER_INTERACTIVE 0x21     // QOS_CLASS_USER_INTERACTIVE

int main() {
    printf("=== Mach Thread Scheduling / QoS Entropy ===\n");
    printf("Samples: %d\n\n", NUM_SAMPLES);

    uint64_t timings[NUM_SAMPLES];
    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // Method 1: thread_info() scheduling statistics micro-variations
    printf("--- Method 1: thread_info() scheduling statistics ---\n");

    thread_basic_info_data_t prev_info, curr_info;
    mach_msg_type_number_t count;

    // Get initial info
    count = THREAD_BASIC_INFO_COUNT;
    thread_info(mach_thread_self(), THREAD_BASIC_INFO,
                (thread_info_t)&prev_info, &count);

    for (int s = 0; s < NUM_SAMPLES; s++) {
        // Do some variable work to change scheduling stats
        volatile uint64_t sink = 0;
        for (int j = 0; j < (s % 100) + 10; j++) sink += j;

        uint64_t t0 = mach_absolute_time();

        // Read thread scheduling info — captures context switch timing
        count = THREAD_BASIC_INFO_COUNT;
        kern_return_t kr = thread_info(mach_thread_self(), THREAD_BASIC_INFO,
                                       (thread_info_t)&curr_info, &count);

        uint64_t t1 = mach_absolute_time();
        timings[s] = t1 - t0;

        // The cpu_usage field changes with each scheduler tick
        // XOR it with the timing for extra entropy
        if (kr == KERN_SUCCESS) {
            timings[s] ^= (uint64_t)curr_info.cpu_usage;
        }

        prev_info = curr_info;
    }

    // Analyze
    int hist1[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        uint64_t v = timings[s];
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(v >> (b * 8));
        hist1[folded]++;
    }
    double sh1 = 0.0; int u1 = 0, m1 = 0;
    for (int i = 0; i < 256; i++) {
        if (hist1[i] > 0) {
            u1++;
            if (hist1[i] > m1) m1 = hist1[i];
            double p = (double)hist1[i] / NUM_SAMPLES;
            sh1 -= p * log2(p);
        }
    }
    printf("thread_info timing: unique=%d Shannon=%.3f Min-H∞=%.3f\n",
           u1, sh1, -log2((double)m1 / NUM_SAMPLES));

    // Method 2: task_info() micro-variations
    printf("\n--- Method 2: task_info() scheduling deltas ---\n");

    task_basic_info_data_t task_prev, task_curr;
    count = TASK_BASIC_INFO_COUNT;
    task_info(mach_task_self(), TASK_BASIC_INFO, (task_info_t)&task_prev, &count);

    uint64_t task_entropy[NUM_SAMPLES];
    for (int s = 0; s < NUM_SAMPLES; s++) {
        // Variable work
        volatile uint64_t sink = 0;
        for (int j = 0; j < (s % 200) + 50; j++) sink += j * j;

        count = TASK_BASIC_INFO_COUNT;
        uint64_t t0 = mach_absolute_time();
        task_info(mach_task_self(), TASK_BASIC_INFO, (task_info_t)&task_curr, &count);
        uint64_t t1 = mach_absolute_time();

        // Combine timing with task scheduling delta
        uint64_t user_delta = (task_curr.user_time.seconds * 1000000 + task_curr.user_time.microseconds)
                            - (task_prev.user_time.seconds * 1000000 + task_prev.user_time.microseconds);
        uint64_t sys_delta = (task_curr.system_time.seconds * 1000000 + task_curr.system_time.microseconds)
                           - (task_prev.system_time.seconds * 1000000 + task_prev.system_time.microseconds);

        task_entropy[s] = (t1 - t0) ^ user_delta ^ (sys_delta << 16) ^ ((uint64_t)task_curr.suspend_count << 32);
        task_prev = task_curr;
    }

    int hist2[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        uint64_t v = task_entropy[s];
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(v >> (b * 8));
        hist2[folded]++;
    }
    double sh2 = 0.0; int u2 = 0, m2 = 0;
    for (int i = 0; i < 256; i++) {
        if (hist2[i] > 0) {
            u2++;
            if (hist2[i] > m2) m2 = hist2[i];
            double p = (double)hist2[i] / NUM_SAMPLES;
            sh2 -= p * log2(p);
        }
    }
    printf("task_info deltas: unique=%d Shannon=%.3f Min-H∞=%.3f\n",
           u2, sh2, -log2((double)m2 / NUM_SAMPLES));

    // Method 3: pthread_setschedparam timing (priority changes)
    printf("\n--- Method 3: Priority change scheduling latency ---\n");

    int policies[] = {SCHED_OTHER, SCHED_RR, SCHED_FIFO};
    int npolicies = sizeof(policies) / sizeof(policies[0]);

    for (int s = 0; s < NUM_SAMPLES; s++) {
        struct sched_param param;
        param.sched_priority = (s % 10) + 1;  // Vary priority
        int policy = policies[s % npolicies];

        uint64_t t0 = mach_absolute_time();

        // Attempt to change scheduling policy — the syscall timing
        // depends on the scheduler's current state
        pthread_setschedparam(pthread_self(), policy, &param);

        // Also yield to let scheduler make a decision
        sched_yield();

        uint64_t t1 = mach_absolute_time();
        timings[s] = t1 - t0;
    }

    // Reset to default
    struct sched_param param = {.sched_priority = 0};
    pthread_setschedparam(pthread_self(), SCHED_OTHER, &param);

    int hist3[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        uint64_t v = timings[s];
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(v >> (b * 8));
        hist3[folded]++;
    }
    double sh3 = 0.0; int u3 = 0, m3 = 0;
    for (int i = 0; i < 256; i++) {
        if (hist3[i] > 0) {
            u3++;
            if (hist3[i] > m3) m3 = hist3[i];
            double p = (double)hist3[i] / NUM_SAMPLES;
            sh3 -= p * log2(p);
        }
    }
    printf("Priority change: unique=%d Shannon=%.3f Min-H∞=%.3f\n",
           u3, sh3, -log2((double)m3 / NUM_SAMPLES));

    // Method 4: getrusage() micro-variations (page faults, context switches)
    printf("\n--- Method 4: getrusage() scheduling entropy ---\n");

    struct rusage ru_prev, ru_curr;
    getrusage(RUSAGE_SELF, &ru_prev);

    for (int s = 0; s < NUM_SAMPLES; s++) {
        volatile uint64_t sink = 0;
        for (int j = 0; j < 500; j++) sink += j;

        uint64_t t0 = mach_absolute_time();
        getrusage(RUSAGE_SELF, &ru_curr);
        uint64_t t1 = mach_absolute_time();

        // Combine timing with rusage deltas
        int64_t nvcsw_delta = ru_curr.ru_nvcsw - ru_prev.ru_nvcsw;
        int64_t nivcsw_delta = ru_curr.ru_nivcsw - ru_prev.ru_nivcsw;
        int64_t minflt_delta = ru_curr.ru_minflt - ru_prev.ru_minflt;

        timings[s] = (t1 - t0) ^ ((uint64_t)nvcsw_delta << 8)
                    ^ ((uint64_t)nivcsw_delta << 16) ^ ((uint64_t)minflt_delta << 24);
        ru_prev = ru_curr;
    }

    int hist4[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        uint64_t v = timings[s];
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(v >> (b * 8));
        hist4[folded]++;
    }
    double sh4 = 0.0; int u4 = 0, m4 = 0;
    for (int i = 0; i < 256; i++) {
        if (hist4[i] > 0) {
            u4++;
            if (hist4[i] > m4) m4 = hist4[i];
            double p = (double)hist4[i] / NUM_SAMPLES;
            sh4 -= p * log2(p);
        }
    }
    printf("rusage deltas: unique=%d Shannon=%.3f Min-H∞=%.3f\n",
           u4, sh4, -log2((double)m4 / NUM_SAMPLES));

    return 0;
}
