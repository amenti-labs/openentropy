// Instruction Retirement Jitter — ARM64 cycle counter entropy
//
// The time to retire a fixed sequence of instructions varies due to:
//   - Pipeline hazards and stalls
//   - Branch predictor state from other processes
//   - Cache hierarchy state (L1/L2 hits vs misses)
//   - TLB state and page table walks
//   - DVFS (Dynamic Voltage and Frequency Scaling) transitions
//   - Interrupt delivery timing
//   - Memory controller arbitration
//
// On Apple Silicon, direct PMU access (PMCCNTR_EL0) is restricted.
// We use CNTVCT_EL0 (virtual timer counter) as a high-resolution
// monotonic counter, plus mach_absolute_time() for comparison.
//
// The entropy comes from the non-deterministic timing of a fixed
// instruction sequence — 1000 NOPs should take constant time on a
// deterministic machine, but doesn't on real hardware.
//
// Build: cc -O2 -o thermal_instruction_retirement thermal_instruction_retirement.c -lm

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>

#define N_SAMPLES 20000
#define NOP_COUNT 1000

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

// Read ARM64 virtual counter (CNTVCT_EL0) — available in user space
static inline uint64_t read_cntvct(void) {
    uint64_t val;
    __asm__ volatile("mrs %0, CNTVCT_EL0" : "=r"(val));
    return val;
}

// Read counter timer frequency
static inline uint64_t read_cntfrq(void) {
    uint64_t val;
    __asm__ volatile("mrs %0, CNTFRQ_EL0" : "=r"(val));
    return val;
}

// Execute N NOPs — the timing should be "constant" but isn't
static inline void execute_nops(void) {
    // 1000 NOPs in groups of 10 for readability
    #define NOP10 "nop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\nnop\n"
    #define NOP100 NOP10 NOP10 NOP10 NOP10 NOP10 NOP10 NOP10 NOP10 NOP10 NOP10
    __asm__ volatile(
        NOP100 NOP100 NOP100 NOP100 NOP100
        NOP100 NOP100 NOP100 NOP100 NOP100
        ::: "memory"
    );
}

// Execute mixed ALU + NOP workload for more pipeline variation
static inline void execute_mixed_workload(volatile uint64_t *sink) {
    uint64_t a = 0x123456789ABCDEF0ULL;
    uint64_t b = 0xFEDCBA9876543210ULL;
    __asm__ volatile(
        "mov x9, %[a]\n"
        "mov x10, %[b]\n"
        // 50 iterations of mixed ALU ops
        ".rept 50\n"
        "add x9, x9, x10\n"
        "eor x10, x10, x9\n"
        "sub x9, x9, #1\n"
        "ror x10, x10, #7\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        "nop\n"
        ".endr\n"
        "str x9, [%[sink]]\n"
        :
        : [a] "r"(a), [b] "r"(b), [sink] "r"(sink)
        : "x9", "x10", "memory"
    );
}

int main(void) {
    printf("# Instruction Retirement Jitter — ARM64 Pipeline Entropy\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    uint64_t cntfrq = read_cntfrq();
    printf("CNTVCT_EL0 frequency: %llu Hz (%.1f MHz)\n", cntfrq, cntfrq / 1e6);
    printf("Timebase: %u/%u\n", tb.numer, tb.denom);
    printf("NOP count per measurement: %d\n", NOP_COUNT);
    printf("Samples: %d\n\n", N_SAMPLES);

    volatile uint64_t sink_val = 0;

    // === Method 1: CNTVCT_EL0 timing of NOP block ===
    printf("=== Method 1: CNTVCT_EL0 NOP timing ===\n");

    uint64_t *cntvct_timings = malloc(N_SAMPLES * sizeof(uint64_t));
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = read_cntvct();
        execute_nops();
        uint64_t t1 = read_cntvct();
        cntvct_timings[i] = t1 - t0;
    }

    uint8_t *c_lsb = malloc(N_SAMPLES);
    uint8_t *c_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        c_lsb[i] = cntvct_timings[i] & 0xFF;
        uint64_t t = cntvct_timings[i];
        c_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                    ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("CNTVCT LSBs", c_lsb, N_SAMPLES);
    analyze_entropy("CNTVCT XOR-fold", c_xor, N_SAMPLES);

    // === Method 2: mach_absolute_time timing of NOP block ===
    printf("\n=== Method 2: mach_absolute_time NOP timing ===\n");

    uint64_t *mach_timings = malloc(N_SAMPLES * sizeof(uint64_t));
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        execute_nops();
        uint64_t t1 = mach_absolute_time();
        mach_timings[i] = t1 - t0;
    }

    uint8_t *m_lsb = malloc(N_SAMPLES);
    uint8_t *m_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        m_lsb[i] = mach_timings[i] & 0xFF;
        uint64_t t = mach_timings[i];
        m_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                    ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("mach LSBs", m_lsb, N_SAMPLES);
    analyze_entropy("mach XOR-fold", m_xor, N_SAMPLES);

    // === Method 3: Mixed workload timing ===
    printf("\n=== Method 3: Mixed ALU+NOP workload ===\n");

    uint64_t *mixed_timings = malloc(N_SAMPLES * sizeof(uint64_t));
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        execute_mixed_workload(&sink_val);
        uint64_t t1 = mach_absolute_time();
        mixed_timings[i] = t1 - t0;
    }

    uint8_t *mx_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t = mixed_timings[i];
        mx_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                     ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("Mixed workload XOR-fold", mx_xor, N_SAMPLES);

    // === Method 4: Counter beat — CNTVCT vs mach_absolute_time ===
    printf("\n=== Method 4: CNTVCT vs mach_absolute_time beat ===\n");

    uint64_t *beat_samples = malloc(N_SAMPLES * sizeof(uint64_t));
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t c = read_cntvct();
        uint64_t m = mach_absolute_time();
        // The ratio should be constant, but LSB jitter reveals clock domain crossing
        beat_samples[i] = c ^ m;
    }

    uint8_t *b_lsb = malloc(N_SAMPLES);
    uint8_t *b_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        b_lsb[i] = beat_samples[i] & 0xFF;
        uint64_t t = beat_samples[i];
        b_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                    ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("Beat LSBs", b_lsb, N_SAMPLES);
    analyze_entropy("Beat XOR-fold", b_xor, N_SAMPLES);

    // === Delta analysis for all methods ===
    printf("\n=== Delta analysis ===\n");
    uint8_t *d1 = malloc(N_SAMPLES - 1);
    uint8_t *d2 = malloc(N_SAMPLES - 1);
    uint8_t *d3 = malloc(N_SAMPLES - 1);

    for (int i = 0; i < N_SAMPLES - 1; i++) {
        int64_t delta;
        uint64_t ud;

        delta = (int64_t)cntvct_timings[i+1] - (int64_t)cntvct_timings[i];
        ud = (uint64_t)delta;
        d1[i] = (ud & 0xFF) ^ ((ud >> 8) & 0xFF);

        delta = (int64_t)mach_timings[i+1] - (int64_t)mach_timings[i];
        ud = (uint64_t)delta;
        d2[i] = (ud & 0xFF) ^ ((ud >> 8) & 0xFF);

        delta = (int64_t)mixed_timings[i+1] - (int64_t)mixed_timings[i];
        ud = (uint64_t)delta;
        d3[i] = (ud & 0xFF) ^ ((ud >> 8) & 0xFF);
    }
    analyze_entropy("CNTVCT delta XOR", d1, N_SAMPLES - 1);
    analyze_entropy("mach delta XOR", d2, N_SAMPLES - 1);
    analyze_entropy("Mixed delta XOR", d3, N_SAMPLES - 1);

    // Statistics
    printf("\n=== Timing statistics ===\n");
    uint64_t sum1 = 0, sum2 = 0, sum3 = 0;
    uint64_t min1 = UINT64_MAX, max1 = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        sum1 += cntvct_timings[i];
        sum2 += mach_timings[i];
        sum3 += mixed_timings[i];
        if (cntvct_timings[i] < min1) min1 = cntvct_timings[i];
        if (cntvct_timings[i] > max1) max1 = cntvct_timings[i];
    }
    printf("  CNTVCT NOP timing: mean=%.1f range=%llu-%llu\n",
           (double)sum1 / N_SAMPLES, min1, max1);
    printf("  mach NOP timing:   mean=%.1f ticks\n", (double)sum2 / N_SAMPLES);
    printf("  Mixed workload:    mean=%.1f ticks\n", (double)sum3 / N_SAMPLES);

    printf("\n  First 20 CNTVCT NOP timings: ");
    for (int i = 0; i < 20; i++) printf("%llu ", cntvct_timings[i]);
    printf("\n  First 20 mach NOP timings:   ");
    for (int i = 0; i < 20; i++) printf("%llu ", mach_timings[i]);
    printf("\n");

    free(cntvct_timings);
    free(mach_timings);
    free(mixed_timings);
    free(beat_samples);
    free(c_lsb); free(c_xor);
    free(m_lsb); free(m_xor);
    free(mx_xor);
    free(b_lsb); free(b_xor);
    free(d1); free(d2); free(d3);
    return 0;
}
