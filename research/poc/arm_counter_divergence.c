// ARM System Counter LSB Divergence
// Apple Silicon has CNTPCT_EL0 (physical counter) and CNTVCT_EL0 (virtual counter).
// We also probe ISB pipeline drain timing and PMU cycle counter differences.
// The key insight: the DIFFERENCE between consecutive counter reads after
// different instruction sequences reveals pipeline state entropy.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <pthread.h>
#include <sys/sysctl.h>

#define N_SAMPLES 20000

static inline uint64_t read_cntvct(void) {
    uint64_t val;
    __asm__ volatile("mrs %0, CNTVCT_EL0" : "=r"(val));
    return val;
}

static inline uint64_t read_cntpct(void) {
    uint64_t val;
    // On macOS, CNTPCT_EL0 may trap. Use CNTVCT_EL0 as primary.
    // mach_absolute_time reads the same counter but through Mach.
    __asm__ volatile("mrs %0, CNTVCT_EL0" : "=r"(val));
    return val;
}

static inline void isb(void) {
    __asm__ volatile("isb" ::: "memory");
}

static inline void dsb(void) {
    __asm__ volatile("dsb sy" ::: "memory");
}

int main(void) {
    printf("# ARM Counter LSB Divergence & Pipeline State Entropy\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // ===== TEST 1: ISB drain time variance =====
    // ISB forces instruction pipeline drain. The drain time depends on:
    // - Number of in-flight instructions
    // - Branch predictor state
    // - Store buffer occupancy
    // - Outstanding cache misses
    printf("=== Test 1: ISB Pipeline Drain Timing ===\n");
    {
        uint64_t timings[N_SAMPLES];
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t t0 = read_cntvct();
            isb();
            uint64_t t1 = read_cntvct();
            timings[i] = t1 - t0;
        }

        int hist[256] = {0};
        for (int i = 0; i < N_SAMPLES; i++) hist[timings[i] & 0xFF]++;
        double sh = 0, me = 0; int mx = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) { double p = (double)hist[i]/N_SAMPLES; sh -= p*log2(p); }
            if (hist[i] > mx) mx = hist[i];
        }
        me = -log2((double)mx/N_SAMPLES);

        // Show distribution
        printf("  Delta distribution (top 10):\n");
        typedef struct { int val; int count; } entry;
        entry entries[256];
        int ne = 0;
        for (int i = 0; i < 256; i++) if (hist[i] > 0) { entries[ne].val = i; entries[ne].count = hist[i]; ne++; }
        for (int i = 0; i < ne-1; i++) for (int j = i+1; j < ne; j++) if (entries[j].count > entries[i].count) { entry t = entries[i]; entries[i] = entries[j]; entries[j] = t; }
        for (int i = 0; i < 10 && i < ne; i++) printf("    delta=%d: %d (%.1f%%)\n", entries[i].val, entries[i].count, 100.0*entries[i].count/N_SAMPLES);

        printf("  Raw LSB: Shannon=%.3f  H∞=%.3f\n\n", sh, me);
    }

    // ===== TEST 2: ISB with varying pipeline loads =====
    // Alternate between different instruction mixes before ISB
    printf("=== Test 2: Variable-load ISB Drain ===\n");
    {
        uint64_t timings[N_SAMPLES];
        volatile uint64_t dummy = 0;

        for (int i = 0; i < N_SAMPLES; i++) {
            // Vary pipeline state before measurement
            switch (i % 5) {
                case 0:
                    // Light: just a few ALU ops
                    dummy += i;
                    break;
                case 1:
                    // Medium: some multiplies
                    dummy *= (i | 1);
                    dummy += dummy >> 17;
                    dummy ^= dummy << 3;
                    break;
                case 2:
                    // Heavy: division (slow on ARM)
                    if (dummy == 0) dummy = 1;
                    dummy = (uint64_t)i / (dummy | 1);
                    break;
                case 3:
                    // Memory: cache-missing loads
                    {
                        volatile char buf[4096];
                        buf[i % 4096] = (char)i;
                        dummy += buf[(i * 2777) % 4096];
                    }
                    break;
                case 4:
                    // Branch-heavy: confuse branch predictor
                    for (int j = 0; j < 8; j++) {
                        if ((i ^ j) & 1) dummy++;
                        else dummy--;
                    }
                    break;
            }

            uint64_t t0 = read_cntvct();
            isb();
            uint64_t t1 = read_cntvct();
            timings[i] = t1 - t0;
        }

        int hist[256] = {0};
        for (int i = 0; i < N_SAMPLES; i++) hist[timings[i] & 0xFF]++;
        double sh = 0; int mx = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) { double p = (double)hist[i]/N_SAMPLES; sh -= p*log2(p); }
            if (hist[i] > mx) mx = hist[i];
        }
        double me = -log2((double)mx/N_SAMPLES);
        printf("  Raw LSB: Shannon=%.3f  H∞=%.3f\n", sh, me);

        // XOR-fold
        int xh[256] = {0};
        for (int i = 0; i < N_SAMPLES; i++) {
            uint8_t f = 0;
            for (int b = 0; b < 8; b++) f ^= (timings[i] >> (b*8)) & 0xFF;
            xh[f]++;
        }
        double xs = 0; int xm = 0;
        for (int i = 0; i < 256; i++) {
            if (xh[i] > 0) { double p = (double)xh[i]/N_SAMPLES; xs -= p*log2(p); }
            if (xh[i] > xm) xm = xh[i];
        }
        printf("  XOR-folded: Shannon=%.3f  H∞=%.3f\n\n", xs, -log2((double)xm/N_SAMPLES));
    }

    // ===== TEST 3: DSB (Data Synchronization Barrier) timing =====
    // DSB waits for all memory operations to complete. Its timing depends on
    // outstanding store buffer entries and cache coherency traffic.
    printf("=== Test 3: DSB Data Barrier Timing ===\n");
    {
        uint64_t timings[N_SAMPLES];
        volatile uint64_t array[1024];

        for (int i = 0; i < N_SAMPLES; i++) {
            // Create some dirty cache lines
            array[i % 1024] = (uint64_t)i * 0xdeadbeef;

            uint64_t t0 = read_cntvct();
            dsb();
            uint64_t t1 = read_cntvct();
            timings[i] = t1 - t0;
        }

        int hist[256] = {0};
        for (int i = 0; i < N_SAMPLES; i++) hist[timings[i] & 0xFF]++;
        double sh = 0; int mx = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) { double p = (double)hist[i]/N_SAMPLES; sh -= p*log2(p); }
            if (hist[i] > mx) mx = hist[i];
        }
        printf("  Raw LSB: Shannon=%.3f  H∞=%.3f\n\n", sh, -log2((double)mx/N_SAMPLES));
    }

    // ===== TEST 4: Consecutive counter read gap variance =====
    // Two back-to-back counter reads. The gap should be constant (~1 tick)
    // but isn't — it reveals pipeline microarchitectural state.
    printf("=== Test 4: Back-to-back Counter Gap ===\n");
    {
        uint64_t gaps[N_SAMPLES];
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t a, b;
            __asm__ volatile(
                "mrs %0, CNTVCT_EL0\n"
                "mrs %1, CNTVCT_EL0\n"
                : "=r"(a), "=r"(b)
            );
            gaps[i] = b - a;
        }

        int hist[256] = {0};
        for (int i = 0; i < N_SAMPLES; i++) hist[gaps[i] & 0xFF]++;
        double sh = 0; int mx = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) { double p = (double)hist[i]/N_SAMPLES; sh -= p*log2(p); }
            if (hist[i] > mx) mx = hist[i];
        }
        printf("  Raw LSB: Shannon=%.3f  H∞=%.3f\n", sh, -log2((double)mx/N_SAMPLES));

        // Show distribution
        printf("  Gap distribution:\n");
        for (int i = 0; i < 16; i++) {
            if (hist[i] > 0) printf("    gap=%d: %d (%.1f%%)\n", i, hist[i], 100.0*hist[i]/N_SAMPLES);
        }
        printf("\n");
    }

    // ===== TEST 5: mach_absolute_time vs CNTVCT_EL0 difference =====
    // These read the "same" counter but through different paths.
    // mach_absolute_time goes through a Mach trap/fast path.
    // Direct MRS reads the register. The path difference creates jitter.
    printf("=== Test 5: mach_absolute_time vs MRS CNTVCT_EL0 ===\n");
    {
        uint64_t diffs[N_SAMPLES];
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t mrs_val = read_cntvct();
            uint64_t mach_val = mach_absolute_time();
            diffs[i] = mach_val - mrs_val;  // should be small positive
        }

        int hist[256] = {0};
        for (int i = 0; i < N_SAMPLES; i++) hist[diffs[i] & 0xFF]++;
        double sh = 0; int mx = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) { double p = (double)hist[i]/N_SAMPLES; sh -= p*log2(p); }
            if (hist[i] > mx) mx = hist[i];
        }
        printf("  Raw LSB: Shannon=%.3f  H∞=%.3f\n", sh, -log2((double)mx/N_SAMPLES));

        // Delta of diffs
        int dh[256] = {0};
        int nd = N_SAMPLES - 1;
        for (int i = 0; i < nd; i++) {
            int64_t d = (int64_t)diffs[i+1] - (int64_t)diffs[i];
            dh[((uint64_t)d) & 0xFF]++;
        }
        double ds = 0; int dm = 0;
        for (int i = 0; i < 256; i++) {
            if (dh[i] > 0) { double p = (double)dh[i]/nd; ds -= p*log2(p); }
            if (dh[i] > dm) dm = dh[i];
        }
        printf("  Delta LSB: Shannon=%.3f  H∞=%.3f\n", ds, -log2((double)dm/nd));

        printf("  First 20 diffs: ");
        for (int i = 0; i < 20; i++) printf("%llu ", diffs[i]);
        printf("\n\n");
    }

    // ===== TEST 6: Cross-core counter read (thread migration jitter) =====
    // Read counter, yield to scheduler, read again. The scheduler may
    // migrate us to a different core (P vs E), causing timing jumps.
    printf("=== Test 6: Scheduler Migration Jitter ===\n");
    {
        uint64_t timings[N_SAMPLES];
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t t0 = read_cntvct();
            sched_yield();
            uint64_t t1 = read_cntvct();
            timings[i] = t1 - t0;
        }

        int hist[256] = {0};
        for (int i = 0; i < N_SAMPLES; i++) {
            uint8_t f = 0;
            for (int b = 0; b < 8; b++) f ^= (timings[i] >> (b*8)) & 0xFF;
            hist[f]++;
        }
        double sh = 0; int mx = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) { double p = (double)hist[i]/N_SAMPLES; sh -= p*log2(p); }
            if (hist[i] > mx) mx = hist[i];
        }
        printf("  XOR-folded: Shannon=%.3f  H∞=%.3f\n", sh, -log2((double)mx/N_SAMPLES));

        // Raw LSB
        int rh[256] = {0};
        for (int i = 0; i < N_SAMPLES; i++) rh[timings[i] & 0xFF]++;
        double rs = 0; int rm = 0;
        for (int i = 0; i < 256; i++) {
            if (rh[i] > 0) { double p = (double)rh[i]/N_SAMPLES; rs -= p*log2(p); }
            if (rh[i] > rm) rm = rh[i];
        }
        printf("  Raw LSB: Shannon=%.3f  H∞=%.3f\n", rs, -log2((double)rm/N_SAMPLES));

        // Delta
        int dh[256] = {0};
        int nd = N_SAMPLES - 1;
        for (int i = 0; i < nd; i++) {
            uint64_t d = timings[i+1] > timings[i] ? timings[i+1] - timings[i] : timings[i] - timings[i+1];
            uint8_t f = 0;
            for (int b = 0; b < 8; b++) f ^= (d >> (b*8)) & 0xFF;
            dh[f]++;
        }
        double ds = 0; int dm = 0;
        for (int i = 0; i < 256; i++) {
            if (dh[i] > 0) { double p = (double)dh[i]/nd; ds -= p*log2(p); }
            if (dh[i] > dm) dm = dh[i];
        }
        printf("  Delta XOR-fold: Shannon=%.3f  H∞=%.3f\n", ds, -log2((double)dm/nd));
    }

    return 0;
}
