/*
 * PoC 2: P-core vs E-core Frequency Drift (Software Ring Oscillator)
 *
 * Physics: Apple M4 has 4 P-cores and 6 E-cores running at different frequencies
 * with independent DVFS (Dynamic Voltage and Frequency Scaling). The exact
 * frequency at any instant depends on thermal state, power budget, and workload.
 *
 * A "software ring oscillator" counts iterations of a tight loop in a fixed time
 * window. The count varies with CPU frequency. By running this simultaneously
 * on P-cores and E-cores, the RATIO between their counts captures the phase
 * relationship between two physically independent clock domains.
 *
 * This is analogous to hardware ring oscillator PUFs, but using the CPU's
 * own frequency instability as the entropy source.
 *
 * Key insight: We DON'T need to know the absolute frequency. The DIFFERENCE
 * in iteration counts between two threads (one on P-core, one on E-core)
 * captures physical frequency jitter that depends on thermal noise in the
 * voltage regulators and PLL circuits.
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <pthread.h>
#include <mach/mach_time.h>
#include <math.h>

#define NUM_SAMPLES 5000
#define MEASUREMENT_NS 1000  // 1 microsecond measurement window

typedef struct {
    volatile uint64_t count;
    volatile int ready;
    volatile int stop;
} thread_data_t;

static void *counter_thread(void *arg) {
    thread_data_t *data = (thread_data_t *)arg;
    data->ready = 1;

    // Spin until told to start (synchronization)
    while (!data->stop && !data->ready) {}

    // Tight counting loop - frequency-dependent
    uint64_t count = 0;
    while (!data->stop) {
        count++;
        count++;
        count++;
        count++;  // Unrolled for tighter loop
    }
    data->count = count;
    return NULL;
}

int main() {
    printf("=== P-core vs E-core Frequency Drift (Software Ring Oscillator) ===\n");
    printf("Samples: %d, Window: %d ns\n\n", NUM_SAMPLES, MEASUREMENT_NS);

    uint64_t timings[NUM_SAMPLES];
    uint64_t counts_main[NUM_SAMPLES];

    // Method 1: Single-thread iteration count variance
    // A tight loop's iteration count in a fixed time window varies with DVFS
    printf("--- Method 1: Single-thread iteration count variance ---\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint64_t t_start = mach_absolute_time();
        uint64_t t_end = t_start + (MEASUREMENT_NS * tb.denom / tb.numer);

        uint64_t count = 0;
        while (mach_absolute_time() < t_end) {
            count++;
        }
        counts_main[s] = count;
        timings[s] = mach_absolute_time() - t_start;
    }

    // Analyze iteration counts
    uint64_t count_min = UINT64_MAX, count_max = 0;
    double count_sum = 0;
    for (int s = 0; s < NUM_SAMPLES; s++) {
        if (counts_main[s] < count_min) count_min = counts_main[s];
        if (counts_main[s] > count_max) count_max = counts_main[s];
        count_sum += counts_main[s];
    }
    printf("Iteration counts: min=%llu max=%llu avg=%.1f range=%llu\n",
           count_min, count_max, count_sum / NUM_SAMPLES, count_max - count_min);

    // Extract entropy from count LSBs
    int hist[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t lsb = (uint8_t)(counts_main[s] & 0xFF);
        hist[lsb]++;
    }

    double shannon = 0.0;
    int unique = 0, max_cnt = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            unique++;
            if (hist[i] > max_cnt) max_cnt = hist[i];
            double p = (double)hist[i] / NUM_SAMPLES;
            shannon -= p * log2(p);
        }
    }
    double min_h = -log2((double)max_cnt / NUM_SAMPLES);

    printf("Count LSB: unique=%d Shannon=%.3f Min-H∞=%.3f\n", unique, shannon, min_h);

    // Method 2: Delta of consecutive counts (removes systematic bias)
    printf("\n--- Method 2: Delta of consecutive iteration counts ---\n");

    int delta_hist[256] = {0};
    for (int s = 1; s < NUM_SAMPLES; s++) {
        int64_t delta = (int64_t)counts_main[s] - (int64_t)counts_main[s-1];
        // XOR-fold the delta
        uint64_t ud = (uint64_t)delta;
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(ud >> (b * 8));
        delta_hist[folded]++;
    }

    double d_shannon = 0.0;
    int d_unique = 0, d_max = 0;
    for (int i = 0; i < 256; i++) {
        if (delta_hist[i] > 0) {
            d_unique++;
            if (delta_hist[i] > d_max) d_max = delta_hist[i];
            double p = (double)delta_hist[i] / (NUM_SAMPLES - 1);
            d_shannon -= p * log2(p);
        }
    }
    double d_min_h = -log2((double)d_max / (NUM_SAMPLES - 1));

    printf("Delta XOR-fold: unique=%d Shannon=%.3f Min-H∞=%.3f\n", d_unique, d_shannon, d_min_h);

    // Method 3: Two-thread race condition (cross-core frequency difference)
    printf("\n--- Method 3: Two-thread iteration race (cross-core DVFS difference) ---\n");

    uint64_t race_diffs[NUM_SAMPLES];
    for (int s = 0; s < NUM_SAMPLES; s++) {
        thread_data_t td1 = {0, 0, 0};
        thread_data_t td2 = {0, 0, 0};

        pthread_t t1, t2;
        pthread_create(&t1, NULL, counter_thread, &td1);
        pthread_create(&t2, NULL, counter_thread, &td2);

        // Wait for threads to be ready
        while (!td1.ready || !td2.ready) {}

        // Let them race for a short window
        uint64_t t_start = mach_absolute_time();
        uint64_t t_end = t_start + (2000 * tb.denom / tb.numer); // 2 microseconds
        while (mach_absolute_time() < t_end) {}

        td1.stop = 1;
        td2.stop = 1;
        pthread_join(t1, NULL);
        pthread_join(t2, NULL);

        // The difference captures cross-core frequency relationship
        race_diffs[s] = td1.count > td2.count ?
                        td1.count - td2.count : td2.count - td1.count;
    }

    // Analyze race differences
    int race_hist[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        uint64_t v = race_diffs[s];
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(v >> (b * 8));
        race_hist[folded]++;
    }

    double r_shannon = 0.0;
    int r_unique = 0, r_max = 0;
    for (int i = 0; i < 256; i++) {
        if (race_hist[i] > 0) {
            r_unique++;
            if (race_hist[i] > r_max) r_max = race_hist[i];
            double p = (double)race_hist[i] / NUM_SAMPLES;
            r_shannon -= p * log2(p);
        }
    }
    double r_min_h = -log2((double)r_max / NUM_SAMPLES);

    printf("Race diff XOR-fold: unique=%d Shannon=%.3f Min-H∞=%.3f\n", r_unique, r_shannon, r_min_h);

    // Show sample diffs
    printf("First 20 race diffs: ");
    for (int s = 0; s < 20; s++) printf("%llu ", race_diffs[s]);
    printf("\n");

    return 0;
}
