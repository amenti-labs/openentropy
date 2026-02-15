// Power Delivery Network Resonance — PCB LC resonance entropy
//
// The PCB power planes have LC resonances at specific frequencies. When different
// chip components draw current, standing waves form in the power delivery network
// creating voltage droops that affect timing of operations.
//
// Approach: Run a known workload on one core while measuring timing on another.
// The timing perturbation captures PDN voltage noise from cross-core coupling.
//
// Build: cc -O2 -o unprecedented_pdn_resonance unprecedented_pdn_resonance.c -lpthread -lm

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <unistd.h>
#include <pthread.h>
#include <mach/mach_time.h>
#include <mach/thread_act.h>
#include <mach/thread_policy.h>

#define N_SAMPLES 12000
#define STRESS_ITERATIONS 1000

static volatile int stress_running = 1;
static volatile uint64_t stress_sink = 0;

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

// Stress thread: high current draw workload to excite PDN resonance
static void *stress_memory_worker(void *arg) {
    (void)arg;
    // Allocate a large buffer to cause cache thrashing and high current draw
    size_t buf_size = 4 * 1024 * 1024; // 4 MB
    volatile uint64_t *buf = malloc(buf_size);
    if (!buf) return NULL;

    while (stress_running) {
        // Stride access pattern — causes high cache miss rate and bursty current
        for (size_t i = 0; i < buf_size / sizeof(uint64_t); i += 64) {
            buf[i] = buf[i] + 1;
        }
        stress_sink = buf[0];
    }
    free((void *)buf);
    return NULL;
}

// Stress thread: integer ALU workload (different current profile)
static void *stress_alu_worker(void *arg) {
    (void)arg;
    while (stress_running) {
        volatile uint64_t a = mach_absolute_time();
        for (int i = 0; i < 10000; i++) {
            a = a * 6364136223846793005ULL + 1442695040888963407ULL;
        }
        stress_sink = a;
    }
    return NULL;
}

// Stress thread: FPU workload (yet another current profile)
static void *stress_fpu_worker(void *arg) {
    (void)arg;
    volatile double acc = 1.0;
    while (stress_running) {
        for (int i = 0; i < 10000; i++) {
            acc = sin(acc) * cos(acc) + 0.1;
        }
    }
    return NULL;
}

// Measurement function: tight timing loop
static void measure_timing(uint64_t *timings, int n, int workload_type) {
    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();

        // Do a fixed small workload and measure how long it takes
        volatile uint64_t acc = 0;
        switch (workload_type) {
            case 0: // NOP-like
                for (int j = 0; j < 100; j++) acc += j;
                break;
            case 1: // Memory reads
                for (int j = 0; j < 100; j++) acc += timings[j % n];
                break;
            case 2: // Mixed
                for (int j = 0; j < 50; j++) {
                    acc += j;
                    acc ^= timings[j % n];
                }
                break;
        }

        uint64_t t1 = mach_absolute_time();
        (void)acc;
        timings[i] = t1 - t0;
    }
}

int main(void) {
    printf("# Power Delivery Network Resonance — Cross-Core Timing Perturbation\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    uint64_t *timings = malloc(N_SAMPLES * sizeof(uint64_t));
    uint8_t *lsbs = malloc(N_SAMPLES);

    // === Test 1: Baseline (no stress) ===
    printf("--- Test 1: Baseline Timing (No Stress) ---\n");
    measure_timing(timings, N_SAMPLES, 0);

    uint64_t tmin = timings[0], tmax = timings[0], tsum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
        tsum += timings[i];
        lsbs[i] = (uint8_t)(timings[i] & 0xFF);
    }
    printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
    analyze_entropy("Baseline LSBs", lsbs, N_SAMPLES);

    // === Test 2: With memory stress (high current, bursty) ===
    printf("\n--- Test 2: Memory Stress (PDN Excitation) ---\n");
    stress_running = 1;
    pthread_t stress_t1;
    pthread_create(&stress_t1, NULL, stress_memory_worker, NULL);
    usleep(10000); // Let stress thread warm up

    measure_timing(timings, N_SAMPLES, 0);
    stress_running = 0;
    pthread_join(stress_t1, NULL);

    tmin = timings[0]; tmax = timings[0]; tsum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
        tsum += timings[i];
        lsbs[i] = (uint8_t)(timings[i] & 0xFF);
    }
    printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
    analyze_entropy("Memory stress LSBs", lsbs, N_SAMPLES);

    // === Test 3: With ALU stress ===
    printf("\n--- Test 3: ALU Stress (Different Current Profile) ---\n");
    stress_running = 1;
    pthread_create(&stress_t1, NULL, stress_alu_worker, NULL);
    usleep(10000);

    measure_timing(timings, N_SAMPLES, 0);
    stress_running = 0;
    pthread_join(stress_t1, NULL);

    tmin = timings[0]; tmax = timings[0]; tsum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
        tsum += timings[i];
        lsbs[i] = (uint8_t)(timings[i] & 0xFF);
    }
    printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
    analyze_entropy("ALU stress LSBs", lsbs, N_SAMPLES);

    // === Test 4: With FPU + memory stress (maximum current draw) ===
    printf("\n--- Test 4: FPU + Memory Stress (Maximum PDN Excitation) ---\n");
    stress_running = 1;
    pthread_t stress_t2, stress_t3;
    pthread_create(&stress_t1, NULL, stress_memory_worker, NULL);
    pthread_create(&stress_t2, NULL, stress_fpu_worker, NULL);
    pthread_create(&stress_t3, NULL, stress_alu_worker, NULL);
    usleep(10000);

    measure_timing(timings, N_SAMPLES, 0);
    stress_running = 0;
    pthread_join(stress_t1, NULL);
    pthread_join(stress_t2, NULL);
    pthread_join(stress_t3, NULL);

    tmin = timings[0]; tmax = timings[0]; tsum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
        tsum += timings[i];
        lsbs[i] = (uint8_t)(timings[i] & 0xFF);
    }
    printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
    analyze_entropy("FPU+Mem+ALU stress LSBs", lsbs, N_SAMPLES);

    // === Test 5: Delta between stressed and unstressed timing ===
    printf("\n--- Test 5: Stress Delta Entropy ---\n");
    // Alternate: measure with stress on/off rapidly
    uint8_t *stress_deltas = malloc(N_SAMPLES);

    stress_running = 1;
    pthread_create(&stress_t1, NULL, stress_memory_worker, NULL);
    pthread_create(&stress_t2, NULL, stress_alu_worker, NULL);
    usleep(10000);

    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        volatile uint64_t acc = 0;
        for (int j = 0; j < 100; j++) acc += j;
        uint64_t t1 = mach_absolute_time();
        (void)acc;
        timings[i] = t1 - t0;
    }
    stress_running = 0;
    pthread_join(stress_t1, NULL);
    pthread_join(stress_t2, NULL);

    for (int i = 1; i < N_SAMPLES; i++) {
        int64_t d = (int64_t)timings[i] - (int64_t)timings[i-1];
        uint64_t ud = (uint64_t)d;
        stress_deltas[i-1] = (uint8_t)((ud >> 0) ^ (ud >> 8) ^ (ud >> 16) ^ (ud >> 24));
    }
    analyze_entropy("Stress delta XOR-fold", stress_deltas, N_SAMPLES - 1);

    free(timings);
    free(lsbs);
    free(stress_deltas);

    printf("\nDone.\n");
    return 0;
}
