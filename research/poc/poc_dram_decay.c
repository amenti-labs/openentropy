/*
 * PoC 1: DRAM Decay Timing
 *
 * Physics: DRAM cells are tiny capacitors that leak charge at physically random
 * rates determined by manufacturing variation and thermal noise. When we allocate
 * memory, write a pattern, then observe how long until the pattern degrades
 * (or observe timing variations in refresh-adjacent operations), we're measuring
 * actual quantum-mechanical tunneling and thermal noise in the DRAM cells.
 *
 * Approach: We can't directly observe cell decay (OS handles refresh), but we CAN
 * measure the timing impact of DRAM refresh interference on our memory accesses.
 * DRAM refresh happens every ~64ms (per bank) and steals memory bus cycles.
 * The exact timing of when our access collides with a refresh is physically random.
 *
 * We measure: tight loop of memory accesses, looking for timing spikes caused
 * by refresh interference. The phase relationship between our access pattern
 * and the refresh cycle is physically nondeterministic.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#include <mach/mach_time.h>

#define REGION_SIZE (64 * 1024 * 1024)  // 64MB - span many DRAM banks/rows
#define NUM_SAMPLES 10000
#define STRIDE 4096  // Page-stride to hit different DRAM rows

static inline uint64_t rdtsc(void) {
    return mach_absolute_time();
}

int main() {
    // Allocate a large region spanning many DRAM banks
    volatile uint8_t *region = (volatile uint8_t *)malloc(REGION_SIZE);
    if (!region) { fprintf(stderr, "malloc failed\n"); return 1; }

    // Initialize to force physical page allocation
    memset((void*)region, 0xAA, REGION_SIZE);

    uint64_t timings[NUM_SAMPLES];
    int num_offsets = REGION_SIZE / STRIDE;

    printf("=== DRAM Refresh Interference Timing ===\n");
    printf("Region: %d MB, Stride: %d bytes, Offsets: %d\n",
           REGION_SIZE / (1024*1024), STRIDE, num_offsets);
    printf("Samples: %d\n\n", NUM_SAMPLES);

    // Warm TLB and cache
    for (int i = 0; i < num_offsets && i < 1000; i++) {
        volatile uint8_t x = region[i * STRIDE];
        (void)x;
    }

    // Measure timing of strided reads across DRAM banks
    // Refresh interference will cause sporadic latency spikes
    uint64_t lcg = rdtsc() | 1;
    for (int s = 0; s < NUM_SAMPLES; s++) {
        // Pseudo-random offset to prevent prefetcher prediction
        lcg = lcg * 6364136223846793005ULL + 1;
        int idx = (lcg >> 32) % num_offsets;

        uint64_t t0 = rdtsc();

        // Read from random DRAM row + write back (RMW forces row buffer operation)
        volatile uint8_t val = region[idx * STRIDE];
        region[idx * STRIDE] = val + 1;

        // Also touch a nearby-but-different bank
        int idx2 = (idx + num_offsets/2) % num_offsets;
        volatile uint8_t val2 = region[idx2 * STRIDE];
        region[idx2 * STRIDE] = val2 + 1;

        uint64_t t1 = rdtsc();
        timings[s] = t1 - t0;
    }

    // Analyze: compute deltas and entropy metrics
    uint64_t min_t = UINT64_MAX, max_t = 0, sum = 0;
    int histogram[256] = {0};

    for (int s = 0; s < NUM_SAMPLES; s++) {
        if (timings[s] < min_t) min_t = timings[s];
        if (timings[s] > max_t) max_t = timings[s];
        sum += timings[s];
    }

    printf("Timing stats: min=%llu max=%llu avg=%.1f range=%llu\n",
           min_t, max_t, (double)sum / NUM_SAMPLES, max_t - min_t);

    // Extract LSBs and compute byte histogram
    int unique_lsbs = 0;
    int lsb_counts[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t lsb = (uint8_t)(timings[s] & 0xFF);
        lsb_counts[lsb]++;
    }
    for (int i = 0; i < 256; i++) {
        if (lsb_counts[i] > 0) unique_lsbs++;
    }

    // XOR-fold to single byte and compute histogram
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint64_t t = timings[s];
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) {
            folded ^= (uint8_t)(t >> (b * 8));
        }
        histogram[folded]++;
    }

    // Compute Shannon entropy
    double shannon = 0.0;
    int unique_vals = 0;
    int max_count = 0;
    for (int i = 0; i < 256; i++) {
        if (histogram[i] > 0) {
            unique_vals++;
            if (histogram[i] > max_count) max_count = histogram[i];
            double p = (double)histogram[i] / NUM_SAMPLES;
            shannon -= p * log2(p);
        }
    }

    // Min-entropy = -log2(max_probability)
    double max_p = (double)max_count / NUM_SAMPLES;
    double min_entropy = -log2(max_p);

    printf("\nXOR-folded byte analysis:\n");
    printf("  Unique values: %d / 256\n", unique_vals);
    printf("  Shannon entropy: %.3f bits/byte\n", shannon);
    printf("  Min-entropy (H∞): %.3f bits/byte\n", min_entropy);
    printf("  Unique LSBs: %d / 256\n", unique_lsbs);

    // Compute delta-based entropy (consecutive timing differences)
    int delta_hist[256] = {0};
    for (int s = 1; s < NUM_SAMPLES; s++) {
        uint64_t delta = timings[s] > timings[s-1] ?
                         timings[s] - timings[s-1] : timings[s-1] - timings[s];
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(delta >> (b * 8));
        delta_hist[folded]++;
    }

    double delta_shannon = 0.0;
    int delta_unique = 0;
    int delta_max = 0;
    for (int i = 0; i < 256; i++) {
        if (delta_hist[i] > 0) {
            delta_unique++;
            if (delta_hist[i] > delta_max) delta_max = delta_hist[i];
            double p = (double)delta_hist[i] / (NUM_SAMPLES - 1);
            delta_shannon -= p * log2(p);
        }
    }
    double delta_min_entropy = -log2((double)delta_max / (NUM_SAMPLES - 1));

    printf("\nDelta-based analysis (consecutive timing differences):\n");
    printf("  Unique delta values: %d / 256\n", delta_unique);
    printf("  Shannon entropy: %.3f bits/byte\n", delta_shannon);
    printf("  Min-entropy (H∞): %.3f bits/byte\n", delta_min_entropy);

    // Show timing distribution (first 20 samples)
    printf("\nFirst 20 raw timings: ");
    for (int s = 0; s < 20 && s < NUM_SAMPLES; s++) {
        printf("%llu ", timings[s]);
    }
    printf("\n");

    free((void*)region);
    return 0;
}
