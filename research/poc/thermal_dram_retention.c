// DRAM Retention Noise — Quantum tunneling in DRAM capacitors
//
// DRAM cells store charge in tiny capacitors (~10 fF). Charge leaks through
// the gate oxide via quantum tunneling — a fundamentally random process.
// While the OS handles DRAM refresh (~64ms per bank), we can observe:
//
// 1. Retention noise: Write a known pattern, busy-wait (keeping CPU busy so
//    the scheduler doesn't sleep), then readback. XOR with original reveals
//    which bits flipped from charge leakage.
//
// 2. Variable Retention Time (VRT): Some cells have metastable traps that
//    cause the retention time to randomly switch between two values.
//
// 3. Read disturb: Reading a DRAM row can disturb charge in adjacent rows
//    (the basis of Rowhammer) — timing of this is physically random.
//
// Note: On modern systems with ECC and aggressive refresh, direct bit flips
// are rare. We primarily measure the TIMING of refresh interference.
//
// Build: cc -O2 -o thermal_dram_retention thermal_dram_retention.c -lm

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <sys/mman.h>
#include <mach/mach_time.h>

#define REGION_SIZE (1024 * 1024)  // 1 MB — spans many DRAM rows
#define DRAM_PAGE_SIZE 4096
#define NUM_PAGES (REGION_SIZE / DRAM_PAGE_SIZE)
#define N_ROUNDS 20
#define N_SAMPLES 10000

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
    printf("# DRAM Retention Noise — Quantum Tunneling PoC\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // Allocate with mmap for page-aligned memory
    volatile uint8_t *region = (volatile uint8_t *)mmap(
        NULL, REGION_SIZE, PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANON, -1, 0);
    if (region == MAP_FAILED) {
        fprintf(stderr, "mmap failed\n");
        return 1;
    }

    // Touch all pages to force physical allocation
    for (int i = 0; i < REGION_SIZE; i += DRAM_PAGE_SIZE) {
        region[i] = 0xAA;
    }

    printf("Region: %d KB (%d pages)\n", REGION_SIZE / 1024, NUM_PAGES);
    printf("Rounds: %d, Samples per round: %d\n\n", N_ROUNDS, N_SAMPLES);

    // === Method 1: Write-Wait-Readback ===
    // Write a known pattern, busy-wait, XOR readback to find changed bits
    printf("=== Method 1: Write-Wait-Readback (retention noise) ===\n");

    int total_flipped_bits = 0;
    int total_flipped_bytes = 0;
    uint8_t *xor_results = malloc(N_ROUNDS * REGION_SIZE);
    int xor_count = 0;

    for (int round = 0; round < N_ROUNDS; round++) {
        // Write known pattern (alternating 0xAA/0x55)
        for (int i = 0; i < REGION_SIZE; i++) {
            region[i] = (i & 1) ? 0x55 : 0xAA;
        }

        // Busy-wait ~10ms (enough for some charge leakage on weak cells)
        uint64_t t0 = mach_absolute_time();
        uint64_t wait_ns = 10000000; // 10ms
        uint64_t wait_ticks = wait_ns * tb.denom / tb.numer;
        while (mach_absolute_time() - t0 < wait_ticks) {
            __asm__ volatile("" ::: "memory");
        }

        // Readback and XOR with expected pattern
        int flipped_bits = 0;
        int flipped_bytes = 0;
        for (int i = 0; i < REGION_SIZE; i++) {
            uint8_t expected = (i & 1) ? 0x55 : 0xAA;
            uint8_t actual = region[i];
            uint8_t diff = actual ^ expected;
            if (diff) {
                flipped_bytes++;
                // Count set bits
                while (diff) { flipped_bits++; diff &= diff - 1; }
            }
            xor_results[xor_count++] = actual ^ expected;
        }
        total_flipped_bits += flipped_bits;
        total_flipped_bytes += flipped_bytes;
    }

    printf("  Total flipped bits: %d across %d rounds\n",
           total_flipped_bits, N_ROUNDS);
    printf("  Total flipped bytes: %d\n", total_flipped_bytes);

    // Even if no bits flipped (likely on modern ECC DRAM), analyze the
    // XOR pattern — all zeros means no direct retention noise observable
    analyze_entropy("XOR residue", (uint8_t *)xor_results,
                    xor_count > N_SAMPLES ? N_SAMPLES : xor_count);

    // === Method 2: Read timing across DRAM rows (refresh interference) ===
    printf("\n=== Method 2: Row-crossing read timing ===\n");

    uint64_t *timings = malloc(N_SAMPLES * sizeof(uint64_t));
    uint64_t lcg = mach_absolute_time() | 1;

    for (int i = 0; i < N_SAMPLES; i++) {
        // Random page offset to hit different DRAM rows
        lcg = lcg * 6364136223846793005ULL + 1;
        int page = (lcg >> 32) % NUM_PAGES;
        int offset = page * DRAM_PAGE_SIZE;

        // Flush cache line to force DRAM access
        __builtin___clear_cache((char *)&region[offset], (char *)&region[offset + 64]);

        uint64_t t0 = mach_absolute_time();

        // Read + write to force actual DRAM row activation
        volatile uint8_t v = region[offset];
        region[offset] = v ^ 0xFF;
        v = region[offset];
        region[offset] = v ^ 0xFF;

        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }

    // Timing analysis
    uint8_t *timing_lsbs = malloc(N_SAMPLES);
    uint8_t *timing_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        timing_lsbs[i] = timings[i] & 0xFF;
        uint64_t t = timings[i];
        timing_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                         ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("Timing LSBs", timing_lsbs, N_SAMPLES);
    analyze_entropy("Timing XOR-fold", timing_xor, N_SAMPLES);

    // Delta timing
    uint8_t *delta_xor = malloc(N_SAMPLES - 1);
    for (int i = 0; i < N_SAMPLES - 1; i++) {
        int64_t d = (int64_t)timings[i+1] - (int64_t)timings[i];
        uint64_t ud = (uint64_t)d;
        delta_xor[i] = (ud & 0xFF) ^ ((ud >> 8) & 0xFF) ^
                        ((ud >> 16) & 0xFF) ^ ((ud >> 24) & 0xFF);
    }
    analyze_entropy("Delta timing XOR-fold", delta_xor, N_SAMPLES - 1);

    // === Method 3: Write-pattern sensitivity ===
    // Different patterns stress different bit lines — entropy varies by pattern
    printf("\n=== Method 3: Pattern-dependent read timing ===\n");
    uint8_t patterns[] = {0x00, 0xFF, 0xAA, 0x55, 0x0F, 0xF0};
    int n_patterns = sizeof(patterns) / sizeof(patterns[0]);

    for (int p = 0; p < n_patterns; p++) {
        // Write pattern
        memset((void *)region, patterns[p], REGION_SIZE);

        uint64_t pat_timings[2000];
        for (int i = 0; i < 2000; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            int page = (lcg >> 32) % NUM_PAGES;
            int offset = page * DRAM_PAGE_SIZE;

            __builtin___clear_cache((char *)&region[offset], (char *)&region[offset + 64]);

            uint64_t t0 = mach_absolute_time();
            volatile uint8_t v = region[offset];
            (void)v;
            uint64_t t1 = mach_absolute_time();
            pat_timings[i] = t1 - t0;
        }

        uint8_t pat_xor[2000];
        for (int i = 0; i < 2000; i++) {
            uint64_t t = pat_timings[i];
            pat_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                          ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
        }

        char label[64];
        snprintf(label, sizeof(label), "Pattern 0x%02X timing XOR", patterns[p]);
        analyze_entropy(label, pat_xor, 2000);
    }

    // Stats
    printf("\n=== Timing statistics ===\n");
    uint64_t tmin = UINT64_MAX, tmax = 0, tsum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
        tsum += timings[i];
    }
    printf("  Min: %llu ticks (%llu ns)\n", tmin, tmin * tb.numer / tb.denom);
    printf("  Max: %llu ticks (%llu ns)\n", tmax, tmax * tb.numer / tb.denom);
    printf("  Mean: %.1f ticks (%.0f ns)\n",
           (double)tsum / N_SAMPLES,
           (double)tsum / N_SAMPLES * tb.numer / tb.denom);

    printf("\n  First 20 row-read timings (ticks): ");
    for (int i = 0; i < 20; i++) printf("%llu ", timings[i]);
    printf("\n");

    munmap((void *)region, REGION_SIZE);
    free(xor_results);
    free(timings);
    free(timing_lsbs);
    free(timing_xor);
    free(delta_xor);
    return 0;
}
