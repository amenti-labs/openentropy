// Mach Thread Quantum Boundary Jitter — Scheduler preemption entropy
//
// The Mach scheduler gives each thread a quantum (typically 10ms). The EXACT
// boundary of when a thread gets preempted depends on:
// - Interrupt timing from all hardware sources
// - Other threads' competing state
// - Timer coalescing decisions
// - The physical interrupt controller (AIC) arbitration
//
// By spinning and detecting preemption events (sudden time jumps), the LSBs
// of preemption timestamps capture interrupt timing noise from ALL hardware.
//
// Build: cc -O2 -o unprecedented_quantum_boundary unprecedented_quantum_boundary.c -lm

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>

#define N_SAMPLES 12000
#define PREEMPT_THRESHOLD 1000  // Ticks — jumps above this indicate preemption

static void analyze_entropy(const char *label, const uint8_t *data, int n) {
    int hist[256] = {0};
    for (int i = 0; i < n; i++) hist[data[i]]++;

    double shannon = 0.0;
    int max_count = 0, unique = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            unique++;
            if (hist[i] > max_count) max_count = hist[i];
            double p = (double)hist[i] / n;
            shannon -= p * log2(p);
        }
    }
    double min_entropy = -log2((double)max_count / n);
    printf("  %s: Shannon=%.3f  H∞=%.3f  unique=%d/256  n=%d\n",
           label, shannon, min_entropy, unique, n);
}

int main(void) {
    printf("# Mach Thread Quantum Boundary Jitter — Scheduler Preemption Entropy\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);
    double ns_per_tick = (double)tb.numer / tb.denom;
    printf("Timer resolution: %.2f ns/tick\n", ns_per_tick);

    // === Test 1: Detect preemption events and record timestamps ===
    printf("\n--- Test 1: Preemption Event Timestamps ---\n");
    uint64_t *preempt_times = malloc(N_SAMPLES * sizeof(uint64_t));
    uint64_t *preempt_durations = malloc(N_SAMPLES * sizeof(uint64_t));
    uint8_t *lsbs = malloc(N_SAMPLES);
    int n_preemptions = 0;

    uint64_t prev = mach_absolute_time();
    uint64_t deadline = prev + (uint64_t)(120e9 / ns_per_tick); // 120 second max

    while (n_preemptions < N_SAMPLES) {
        uint64_t now = mach_absolute_time();
        uint64_t delta = now - prev;

        if (now > deadline) {
            printf("  Timeout after 120s, got %d preemptions\n", n_preemptions);
            break;
        }

        if (delta > PREEMPT_THRESHOLD) {
            // Preemption detected!
            preempt_times[n_preemptions] = now;
            preempt_durations[n_preemptions] = delta;
            n_preemptions++;
        }
        prev = now;
    }

    if (n_preemptions < 100) {
        printf("  Too few preemptions detected (%d). Adjusting threshold...\n", n_preemptions);
        // Try lower threshold
        n_preemptions = 0;
        prev = mach_absolute_time();
        deadline = prev + (uint64_t)(30e9 / ns_per_tick);
        while (n_preemptions < N_SAMPLES && mach_absolute_time() < deadline) {
            uint64_t now = mach_absolute_time();
            uint64_t delta = now - prev;
            if (delta > 100) { // Very low threshold
                preempt_times[n_preemptions] = now;
                preempt_durations[n_preemptions] = delta;
                n_preemptions++;
            }
            prev = now;
        }
        printf("  With threshold=100: got %d events\n", n_preemptions);
    }

    if (n_preemptions > 0) {
        // Show duration stats
        uint64_t dmin = preempt_durations[0], dmax = preempt_durations[0], dsum = 0;
        for (int i = 0; i < n_preemptions; i++) {
            if (preempt_durations[i] < dmin) dmin = preempt_durations[i];
            if (preempt_durations[i] > dmax) dmax = preempt_durations[i];
            dsum += preempt_durations[i];
        }
        printf("  Preemption duration range: %llu - %llu ticks (%.0f - %.0f ns), mean=%llu\n",
               dmin, dmax, dmin * ns_per_tick, dmax * ns_per_tick, dsum/n_preemptions);

        // Extract entropy from preemption timestamps
        for (int i = 0; i < n_preemptions; i++) {
            lsbs[i] = (uint8_t)(preempt_times[i] & 0xFF);
        }
        analyze_entropy("Preemption timestamp LSBs", lsbs, n_preemptions);

        // Extract entropy from preemption durations
        for (int i = 0; i < n_preemptions; i++) {
            lsbs[i] = (uint8_t)(preempt_durations[i] & 0xFF);
        }
        analyze_entropy("Preemption duration LSBs", lsbs, n_preemptions);

        // XOR-fold durations
        for (int i = 0; i < n_preemptions; i++) {
            uint64_t d = preempt_durations[i];
            lsbs[i] = (uint8_t)((d >> 0) ^ (d >> 8) ^ (d >> 16) ^ (d >> 24) ^
                                 (d >> 32) ^ (d >> 40) ^ (d >> 48) ^ (d >> 56));
        }
        analyze_entropy("Preemption duration XOR-fold", lsbs, n_preemptions);
    }

    // === Test 2: Inter-preemption interval ===
    printf("\n--- Test 2: Inter-Preemption Interval ---\n");
    if (n_preemptions > 1) {
        uint8_t *intervals = malloc(n_preemptions);
        for (int i = 1; i < n_preemptions; i++) {
            uint64_t interval = preempt_times[i] - preempt_times[i-1];
            // XOR-fold to byte
            intervals[i-1] = (uint8_t)((interval >> 0) ^ (interval >> 8) ^
                                        (interval >> 16) ^ (interval >> 24));
        }
        analyze_entropy("Inter-preemption interval XOR-fold", intervals, n_preemptions - 1);
        free(intervals);
    }

    // === Test 3: Continuous spinning with fine-grained timing ===
    printf("\n--- Test 3: Continuous Spin Timing LSBs ---\n");
    // Capture ALL timing deltas, not just preemptions
    uint8_t *spin_data = malloc(N_SAMPLES);
    prev = mach_absolute_time();
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t now = mach_absolute_time();
        uint64_t delta = now - prev;
        spin_data[i] = (uint8_t)(delta & 0xFF);
        prev = now;
        // Small busy-wait to avoid getting just 0/1 deltas
        for (volatile int j = 0; j < 50; j++) {}
    }
    analyze_entropy("Spin timing LSBs", spin_data, N_SAMPLES);

    // === Test 4: XOR of timestamp with preemption-aware counter ===
    printf("\n--- Test 4: Timestamp XOR Counter ---\n");
    uint64_t counter = 0;
    prev = mach_absolute_time();
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t now = mach_absolute_time();
        uint64_t delta = now - prev;
        counter++;
        // XOR counter with timestamp — counter is deterministic,
        // timestamp captures all hardware nondeterminism
        uint64_t xored = counter ^ now;
        spin_data[i] = (uint8_t)((xored >> 0) ^ (xored >> 8) ^
                                  (xored >> 16) ^ (xored >> 24));
        prev = now;
        for (volatile int j = 0; j < 50; j++) {}
    }
    analyze_entropy("Timestamp XOR counter", spin_data, N_SAMPLES);

    free(preempt_times);
    free(preempt_durations);
    free(lsbs);
    free(spin_data);

    printf("\nDone.\n");
    return 0;
}
