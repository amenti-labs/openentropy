/*
 * PoC 6: NUMA-like Memory Asymmetry on Apple Silicon
 *
 * Physics: Apple M4's unified memory architecture is NOT truly uniform.
 * Different cores access different physical memory addresses with different
 * latencies because:
 * 1. P-core and E-core clusters have different L2 cache sizes
 * 2. The SLC (System Level Cache) has physical banking that creates
 *    address-dependent latency
 * 3. The memory controller has multiple channels with interleaving
 *
 * By measuring memory access latency from different addresses and comparing
 * the VARIANCE across addresses, we can observe the physical memory topology.
 * The key insight is that the RELATIVE ordering of fast/slow addresses
 * changes over time due to SLC evictions from GPU/ANE/ISP activity.
 *
 * Also tests: atomic operation contention timing across multiple threads
 * which exercises the coherence engine's arbitration.
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <pthread.h>
#include <math.h>
#include <mach/mach_time.h>
#include <stdatomic.h>

#define NUM_SAMPLES 10000
#define REGION_SIZE (32 * 1024 * 1024)  // 32MB

// Method: Atomic CAS (Compare-And-Swap) contention
// Multiple threads racing on atomic operations creates physically
// nondeterministic arbitration timing
typedef struct {
    _Atomic uint64_t *targets;
    int num_targets;
    volatile int start;
    volatile int stop;
    uint64_t *timings;
    int count;
} cas_data_t;

static void *cas_thread(void *arg) {
    cas_data_t *data = (cas_data_t *)arg;
    while (!data->start) { __asm__ volatile("yield"); }

    int t = 0;
    uint64_t lcg = mach_absolute_time() | 1;
    while (!data->stop && t < data->count) {
        lcg = lcg * 6364136223846793005ULL + 1;
        int idx = (lcg >> 32) % data->num_targets;

        uint64_t t0 = mach_absolute_time();

        // Attempt CAS — the arbitration between threads is physically random
        uint64_t expected = atomic_load_explicit(&data->targets[idx], memory_order_relaxed);
        atomic_compare_exchange_weak_explicit(
            &data->targets[idx], &expected, expected + 1,
            memory_order_acq_rel, memory_order_relaxed);

        uint64_t t1 = mach_absolute_time();
        data->timings[t++] = t1 - t0;
    }
    data->count = t;
    return NULL;
}

int main() {
    printf("=== Atomic CAS Contention Entropy ===\n\n");

    // Allocate contention targets (spread across cache lines)
    int num_targets = 256;
    _Atomic uint64_t *targets = aligned_alloc(128,
        num_targets * 128);  // 128-byte spacing (Apple cache line)
    memset((void*)targets, 0, num_targets * 128);

    // Method 1: Single-thread CAS (baseline)
    printf("--- Method 1: Single-thread CAS (baseline) ---\n");
    uint64_t timings[NUM_SAMPLES];
    uint64_t lcg = mach_absolute_time() | 1;

    for (int s = 0; s < NUM_SAMPLES; s++) {
        lcg = lcg * 6364136223846793005ULL + 1;
        int idx = (lcg >> 32) % num_targets;

        uint64_t t0 = mach_absolute_time();
        uint64_t expected = atomic_load(&targets[idx * 16]); // *16 for 128B spacing
        atomic_compare_exchange_weak(&targets[idx * 16], &expected, expected + 1);
        uint64_t t1 = mach_absolute_time();
        timings[s] = t1 - t0;
    }

    int hist[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(timings[s] >> (b * 8));
        hist[folded]++;
    }
    double sh = 0.0; int u = 0, mx = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) { u++; if (hist[i] > mx) mx = hist[i];
            double p = (double)hist[i] / NUM_SAMPLES; sh -= p * log2(p); }
    }
    printf("Single CAS: unique=%d Shannon=%.3f Min-H∞=%.3f\n", u, sh, -log2((double)mx / NUM_SAMPLES));

    // Method 2: 4-thread CAS contention
    printf("\n--- Method 2: 4-thread CAS contention ---\n");

    int nthreads = 4;
    cas_data_t tdata[4];
    pthread_t threads[4];

    // Reallocate targets for contention (each entry is 128B apart)
    _Atomic uint64_t *cas_targets = aligned_alloc(128, 64 * 128);
    memset((void*)cas_targets, 0, 64 * 128);

    for (int i = 0; i < nthreads; i++) {
        tdata[i].targets = cas_targets;
        tdata[i].num_targets = 64;
        tdata[i].start = 0;
        tdata[i].stop = 0;
        tdata[i].timings = calloc(NUM_SAMPLES, sizeof(uint64_t));
        tdata[i].count = NUM_SAMPLES;
        pthread_create(&threads[i], NULL, cas_thread, &tdata[i]);
    }

    // Start all threads
    for (int i = 0; i < nthreads; i++) tdata[i].start = 1;

    // Let them run for a bit
    uint64_t start = mach_absolute_time();
    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);
    while ((mach_absolute_time() - start) * tb.numer / tb.denom < 50000000) {} // 50ms

    for (int i = 0; i < nthreads; i++) tdata[i].stop = 1;
    for (int i = 0; i < nthreads; i++) pthread_join(threads[i], NULL);

    // Analyze thread 0's timings (most affected by contention)
    int n = tdata[0].count;
    printf("Thread 0 collected %d samples\n", n);

    if (n > 100) {
        memset(hist, 0, sizeof(hist));
        for (int s = 0; s < n; s++) {
            uint8_t folded = 0;
            for (int b = 0; b < 8; b++) folded ^= (uint8_t)(tdata[0].timings[s] >> (b * 8));
            hist[folded]++;
        }
        sh = 0.0; u = 0; mx = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) { u++; if (hist[i] > mx) mx = hist[i];
                double p = (double)hist[i] / n; sh -= p * log2(p); }
        }
        printf("4-thread CAS: unique=%d Shannon=%.3f Min-H∞=%.3f\n", u, sh, -log2((double)mx / n));

        // XOR all 4 threads' timings together for combined entropy
        int min_n = n;
        for (int i = 1; i < nthreads; i++)
            if (tdata[i].count < min_n) min_n = tdata[i].count;

        memset(hist, 0, sizeof(hist));
        for (int s = 0; s < min_n; s++) {
            uint64_t combined = 0;
            for (int i = 0; i < nthreads; i++)
                combined ^= tdata[i].timings[s];
            uint8_t folded = 0;
            for (int b = 0; b < 8; b++) folded ^= (uint8_t)(combined >> (b * 8));
            hist[folded]++;
        }
        sh = 0.0; u = 0; mx = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) { u++; if (hist[i] > mx) mx = hist[i];
                double p = (double)hist[i] / min_n; sh -= p * log2(p); }
        }
        printf("4-thread XOR combined: unique=%d Shannon=%.3f Min-H∞=%.3f\n", u, sh, -log2((double)mx / min_n));
    }

    // Method 3: Memory latency landscape (address-dependent timing)
    printf("\n--- Method 3: Memory latency landscape ---\n");

    volatile uint8_t *region = malloc(REGION_SIZE);
    memset((void*)region, 0xAA, REGION_SIZE);

    // Measure access time at many different addresses
    for (int s = 0; s < NUM_SAMPLES; s++) {
        lcg = lcg * 6364136223846793005ULL + 1;
        size_t offset1 = (lcg >> 16) % (REGION_SIZE - 64);
        lcg = lcg * 6364136223846793005ULL + 1;
        size_t offset2 = (lcg >> 16) % (REGION_SIZE - 64);

        // Flush cache for these addresses (via reading far-apart memory)
        volatile uint8_t sink = 0;
        for (int i = 0; i < 16; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            sink += region[(lcg >> 16) % REGION_SIZE];
        }

        uint64_t t0 = mach_absolute_time();
        sink += region[offset1];
        sink += region[offset2];
        __asm__ volatile("dmb sy");  // Full memory barrier
        uint64_t t1 = mach_absolute_time();

        timings[s] = t1 - t0;
        (void)sink;
    }

    memset(hist, 0, sizeof(hist));
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(timings[s] >> (b * 8));
        hist[folded]++;
    }
    sh = 0.0; u = 0; mx = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) { u++; if (hist[i] > mx) mx = hist[i];
            double p = (double)hist[i] / NUM_SAMPLES; sh -= p * log2(p); }
    }
    printf("Memory landscape: unique=%d Shannon=%.3f Min-H∞=%.3f\n", u, sh, -log2((double)mx / NUM_SAMPLES));

    // Cleanup
    free((void*)region);
    free((void*)targets);
    free((void*)cas_targets);
    for (int i = 0; i < nthreads; i++) free(tdata[i].timings);

    return 0;
}
