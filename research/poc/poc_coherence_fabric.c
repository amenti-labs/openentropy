/*
 * PoC 3: Cache Coherence Fabric Timing (ICE - Interconnect Coherence Engine)
 *
 * Physics: Apple M4's ICE handles cache coherency between P-core cluster,
 * E-core cluster, GPU, and ANE. When two cores share a cache line, the
 * MESI protocol (Modified/Exclusive/Shared/Invalid) generates coherence
 * traffic across the fabric. The latency of this traffic depends on:
 *
 * 1. Which cores are involved (P↔P, P↔E, same cluster vs cross-cluster)
 * 2. Current fabric load from GPU, ANE, ISP, display engine
 * 3. SLC (System Level Cache) state
 * 4. Directory-based coherence protocol state
 *
 * By forcing cache line bouncing between threads on different core types
 * and measuring the transition latency, we observe the coherence fabric's
 * nondeterministic timing — a previously unexploited entropy domain.
 *
 * This is NOT the same as our existing cache_contention source, which
 * measures L1/L2 miss patterns. This measures the COHERENCE PROTOCOL
 * specifically — the inter-cluster communication fabric.
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <pthread.h>
#include <mach/mach_time.h>
#include <math.h>

#define CACHELINE_SIZE 128  // Apple Silicon uses 128-byte cache lines
#define NUM_SAMPLES 10000
#define NUM_LINES 64        // Number of cache lines to bounce

// Cache-line-aligned shared data
typedef struct __attribute__((aligned(128))) {
    volatile uint64_t value;
    char _pad[128 - sizeof(uint64_t)];
} aligned_line_t;

typedef struct {
    aligned_line_t *lines;
    int num_lines;
    volatile int *phase;  // Coordination between threads
    volatile int start;
    volatile int stop;
    uint64_t *timings;
    int timing_count;
} bounce_data_t;

static void *bouncer_thread(void *arg) {
    bounce_data_t *data = (bounce_data_t *)arg;

    // Wait for start signal
    while (!data->start) { __asm__ volatile("yield"); }

    int t = 0;
    while (!data->stop && t < data->timing_count) {
        // Wait for our turn (odd phase = this thread's turn)
        while ((*data->phase & 1) == 0 && !data->stop) {
            __asm__ volatile("yield");
        }
        if (data->stop) break;

        uint64_t t0 = mach_absolute_time();

        // Touch all cache lines — forces coherence traffic
        for (int i = 0; i < data->num_lines; i++) {
            data->lines[i].value++;  // Write forces MESI transition
        }

        uint64_t t1 = mach_absolute_time();
        data->timings[t++] = t1 - t0;

        // Signal other thread
        __atomic_fetch_add(data->phase, 1, __ATOMIC_SEQ_CST);
    }
    data->timing_count = t;
    return NULL;
}

int main() {
    printf("=== Cache Coherence Fabric (ICE) Timing ===\n");
    printf("Cache line size: %d bytes, Lines: %d, Samples: %d\n\n",
           CACHELINE_SIZE, NUM_LINES, NUM_SAMPLES);

    // Allocate cache-line-aligned shared data
    aligned_line_t *lines = (aligned_line_t *)aligned_alloc(128,
        NUM_LINES * sizeof(aligned_line_t));
    if (!lines) { fprintf(stderr, "aligned_alloc failed\n"); return 1; }
    memset(lines, 0, NUM_LINES * sizeof(aligned_line_t));

    volatile int phase = 0;

    uint64_t *timings_main = calloc(NUM_SAMPLES, sizeof(uint64_t));
    uint64_t *timings_remote = calloc(NUM_SAMPLES, sizeof(uint64_t));

    bounce_data_t remote_data = {
        .lines = lines,
        .num_lines = NUM_LINES,
        .phase = &phase,
        .start = 0,
        .stop = 0,
        .timings = timings_remote,
        .timing_count = NUM_SAMPLES,
    };

    pthread_t remote_thread;
    pthread_create(&remote_thread, NULL, bouncer_thread, &remote_data);

    // Start the bouncing
    remote_data.start = 1;

    int t = 0;
    for (int s = 0; s < NUM_SAMPLES && !remote_data.stop; s++) {
        // Wait for our turn (even phase = main thread's turn)
        while ((phase & 1) != 0 && !remote_data.stop) {
            __asm__ volatile("yield");
        }

        uint64_t t0 = mach_absolute_time();

        // Touch all cache lines — forces coherence transitions back to us
        for (int i = 0; i < NUM_LINES; i++) {
            lines[i].value++;
        }

        uint64_t t1 = mach_absolute_time();
        timings_main[t++] = t1 - t0;

        // Signal other thread
        __atomic_fetch_add((volatile int *)&phase, 1, __ATOMIC_SEQ_CST);
    }

    remote_data.stop = 1;
    pthread_join(remote_thread, NULL);

    int main_count = t;
    int remote_count = remote_data.timing_count;

    printf("Collected: main=%d remote=%d samples\n\n", main_count, remote_count);

    // Analyze main thread coherence timings
    printf("--- Main thread coherence acquisition timing ---\n");

    uint64_t min_t = UINT64_MAX, max_t = 0;
    double sum = 0;
    for (int s = 0; s < main_count; s++) {
        if (timings_main[s] < min_t) min_t = timings_main[s];
        if (timings_main[s] > max_t) max_t = timings_main[s];
        sum += timings_main[s];
    }
    printf("Stats: min=%llu max=%llu avg=%.1f range=%llu\n",
           min_t, max_t, sum / main_count, max_t - min_t);

    // XOR-fold and compute entropy
    int hist[256] = {0};
    for (int s = 0; s < main_count; s++) {
        uint64_t v = timings_main[s];
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(v >> (b * 8));
        hist[folded]++;
    }

    double shannon = 0.0;
    int unique = 0, max_cnt = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            unique++;
            if (hist[i] > max_cnt) max_cnt = hist[i];
            double p = (double)hist[i] / main_count;
            shannon -= p * log2(p);
        }
    }
    double min_h = -log2((double)max_cnt / main_count);

    printf("XOR-fold: unique=%d Shannon=%.3f Min-H∞=%.3f\n", unique, shannon, min_h);

    // Delta analysis
    int dhist[256] = {0};
    for (int s = 1; s < main_count; s++) {
        uint64_t delta = timings_main[s] > timings_main[s-1] ?
                         timings_main[s] - timings_main[s-1] :
                         timings_main[s-1] - timings_main[s];
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(delta >> (b * 8));
        dhist[folded]++;
    }

    double d_shannon = 0.0;
    int d_unique = 0, d_max = 0;
    for (int i = 0; i < 256; i++) {
        if (dhist[i] > 0) {
            d_unique++;
            if (dhist[i] > d_max) d_max = dhist[i];
            double p = (double)dhist[i] / (main_count - 1);
            d_shannon -= p * log2(p);
        }
    }
    double d_min_h = -log2((double)d_max / (main_count - 1));

    printf("Delta XOR-fold: unique=%d Shannon=%.3f Min-H∞=%.3f\n", d_unique, d_shannon, d_min_h);

    // Analyze the DIFFERENCE between main and remote thread timings
    // This captures the asymmetry of coherence traffic direction
    printf("\n--- Cross-thread coherence asymmetry ---\n");
    int min_both = main_count < remote_count ? main_count : remote_count;
    int ahist[256] = {0};
    for (int s = 0; s < min_both; s++) {
        uint64_t diff = timings_main[s] > timings_remote[s] ?
                        timings_main[s] - timings_remote[s] :
                        timings_remote[s] - timings_main[s];
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(diff >> (b * 8));
        ahist[folded]++;
    }

    double a_shannon = 0.0;
    int a_unique = 0, a_max = 0;
    for (int i = 0; i < 256; i++) {
        if (ahist[i] > 0) {
            a_unique++;
            if (ahist[i] > a_max) a_max = ahist[i];
            double p = (double)ahist[i] / min_both;
            a_shannon -= p * log2(p);
        }
    }
    double a_min_h = -log2((double)a_max / min_both);

    printf("Asymmetry XOR-fold: unique=%d Shannon=%.3f Min-H∞=%.3f\n", a_unique, a_shannon, a_min_h);

    printf("\nFirst 20 main timings: ");
    for (int s = 0; s < 20 && s < main_count; s++) printf("%llu ", timings_main[s]);
    printf("\n");

    free(lines);
    free(timings_main);
    free(timings_remote);
    return 0;
}
