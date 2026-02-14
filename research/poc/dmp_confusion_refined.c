// DMP (Data Memory-dependent Prefetcher) Confusion — Refined PoC
// This is the best candidate from initial testing: H∞ = 3.033
//
// Apple's DMP reads memory VALUES and interprets them as pointers for
// prefetching. By creating arrays filled with valid-looking pointer values
// and accessing them in unpredictable patterns, we measure the DMP's
// prediction failure latency — a genuinely novel entropy domain.
//
// This is fundamentally different from cache timing: we're measuring
// the DMP's WRONG PREDICTIONS, not cache hits/misses themselves.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <sys/mman.h>
#include <pthread.h>

#define N_SAMPLES 50000
#define ARRAY_SIZE (16 * 1024 * 1024)

static inline uint64_t read_counter(void) {
    uint64_t val;
    __asm__ volatile("isb\nmrs %0, CNTVCT_EL0" : "=r"(val));
    return val;
}

static inline void memory_barrier(void) {
    __asm__ volatile("dmb sy" ::: "memory");
}

static void analyze(const char *name, uint64_t *timings, int n) {
    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // Raw LSB
    int rh[256] = {0};
    for (int i = 0; i < n; i++) rh[timings[i] & 0xFF]++;
    double rs = 0; int rm = 0;
    for (int i = 0; i < 256; i++) {
        if (rh[i] > 0) { double p = (double)rh[i]/n; rs -= p*log2(p); }
        if (rh[i] > rm) rm = rh[i];
    }

    // XOR-fold
    int xh[256] = {0};
    for (int i = 0; i < n; i++) {
        uint8_t f = 0;
        for (int b = 0; b < 8; b++) f ^= (timings[i] >> (b*8)) & 0xFF;
        xh[f]++;
    }
    double xs = 0; int xm = 0;
    for (int i = 0; i < 256; i++) {
        if (xh[i] > 0) { double p = (double)xh[i]/n; xs -= p*log2(p); }
        if (xh[i] > xm) xm = xh[i];
    }

    // Delta XOR-fold
    int dh[256] = {0};
    int nd = n - 1;
    for (int i = 0; i < nd; i++) {
        int64_t d = (int64_t)timings[i+1] - (int64_t)timings[i];
        uint8_t f = 0;
        for (int b = 0; b < 8; b++) f ^= (((uint64_t)d) >> (b*8)) & 0xFF;
        dh[f]++;
    }
    double ds = 0; int dm = 0;
    for (int i = 0; i < 256; i++) {
        if (dh[i] > 0) { double p = (double)dh[i]/nd; ds -= p*log2(p); }
        if (dh[i] > dm) dm = dh[i];
    }

    // XOR adjacent timings, then fold
    int ah[256] = {0};
    int na = n - 1;
    for (int i = 0; i < na; i++) {
        uint64_t x = timings[i] ^ timings[i+1];
        uint8_t f = 0;
        for (int b = 0; b < 8; b++) f ^= (x >> (b*8)) & 0xFF;
        ah[f]++;
    }
    double as_sh = 0; int am = 0;
    for (int i = 0; i < 256; i++) {
        if (ah[i] > 0) { double p = (double)ah[i]/na; as_sh -= p*log2(p); }
        if (ah[i] > am) am = ah[i];
    }

    uint64_t sum = 0, tmin = UINT64_MAX, tmax = 0;
    for (int i = 0; i < n; i++) {
        sum += timings[i]; if (timings[i] < tmin) tmin = timings[i]; if (timings[i] > tmax) tmax = timings[i];
    }
    double mean = (double)sum / n;
    uint64_t mns = (uint64_t)(mean * tb.numer / tb.denom);

    printf("%s:\n", name);
    printf("  N=%d  Mean=%.0f ticks (≈%llu ns)  Range=%llu-%llu\n", n, mean, mns, tmin, tmax);
    printf("  Raw LSB:        Shannon=%.3f  H∞=%.3f\n", rs, -log2((double)rm/n));
    printf("  XOR-folded:     Shannon=%.3f  H∞=%.3f\n", xs, -log2((double)xm/n));
    printf("  Delta XOR-fold: Shannon=%.3f  H∞=%.3f\n", ds, -log2((double)dm/nd));
    printf("  XOR-adj fold:   Shannon=%.3f  H∞=%.3f\n\n", as_sh, -log2((double)am/na));
}

int main(void) {
    printf("# DMP Confusion — Refined (50K samples)\n\n");

    uint64_t *array = mmap(NULL, ARRAY_SIZE, PROT_READ | PROT_WRITE,
                           MAP_PRIVATE | MAP_ANON, -1, 0);
    if (array == MAP_FAILED) { perror("mmap"); return 1; }

    uint64_t base = (uint64_t)array;
    size_t n_elements = ARRAY_SIZE / sizeof(uint64_t);
    uint64_t lcg = mach_absolute_time() | 1;

    // Fill with pointer-like values
    for (size_t i = 0; i < n_elements; i++) {
        lcg = lcg * 6364136223846793005ULL + 1;
        size_t offset = (lcg >> 16) % n_elements;
        array[i] = base + offset * sizeof(uint64_t);
    }

    // ===== VARIANT A: Pure DMP confusion (original best approach) =====
    {
        uint64_t timings[N_SAMPLES];
        volatile uint64_t sink = 0;

        for (int i = 0; i < N_SAMPLES; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t idx = (lcg >> 16) % (n_elements - 256);

            memory_barrier();
            uint64_t t0 = read_counter();

            uint64_t val = array[idx];
            size_t next = (val - base) / sizeof(uint64_t);
            if (next < n_elements) sink += array[next];
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t surprise = (lcg >> 16) % n_elements;
            sink += array[surprise];

            memory_barrier();
            uint64_t t1 = read_counter();
            timings[i] = t1 - t0;
        }
        analyze("DMP Confusion (standard)", timings, N_SAMPLES);
    }

    // ===== VARIANT B: Triple-hop with direction reversal =====
    {
        uint64_t timings[N_SAMPLES];
        volatile uint64_t sink = 0;

        for (int i = 0; i < N_SAMPLES; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t idx = (lcg >> 16) % (n_elements - 256);

            memory_barrier();
            uint64_t t0 = read_counter();

            // Triple pointer chase + reversal
            uint64_t val = array[idx];
            size_t next = (val - base) / sizeof(uint64_t);
            if (next < n_elements) {
                uint64_t val2 = array[next];
                size_t next2 = (val2 - base) / sizeof(uint64_t);
                if (next2 < n_elements) {
                    sink += array[next2];
                    // Now reverse direction — DMP predicted forward
                    sink += array[idx > 64 ? idx - 64 : 0];
                }
            }

            memory_barrier();
            uint64_t t1 = read_counter();
            timings[i] = t1 - t0;
        }
        analyze("DMP Triple-hop Reversal", timings, N_SAMPLES);
    }

    // ===== VARIANT C: Alternating stride with DMP bait =====
    {
        uint64_t timings[N_SAMPLES];
        volatile uint64_t sink = 0;

        for (int i = 0; i < N_SAMPLES; i++) {
            // Alternately train and confuse the DMP
            if (i % 2 == 0) {
                // "Train" phase: sequential pointer chase (DMP learns pattern)
                size_t idx = (i / 2) % (n_elements - 4);
                uint64_t val = array[idx];
                size_t next = (val - base) / sizeof(uint64_t);
                if (next < n_elements) sink += array[next];
            }

            memory_barrier();
            uint64_t t0 = read_counter();

            // "Confuse" phase: completely random access (DMP gets it wrong)
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t rnd1 = (lcg >> 16) % n_elements;
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t rnd2 = (lcg >> 16) % n_elements;
            sink += array[rnd1];
            sink += array[rnd2];

            memory_barrier();
            uint64_t t1 = read_counter();
            timings[i] = t1 - t0;
        }
        analyze("DMP Train-Confuse Alternation", timings, N_SAMPLES);
    }

    // ===== VARIANT D: Cross-page DMP confusion (4KB boundary crossing) =====
    {
        uint64_t timings[N_SAMPLES];
        volatile uint64_t sink = 0;
        size_t page_elements = 4096 / sizeof(uint64_t);  // 512 elements per page

        for (int i = 0; i < N_SAMPLES; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            // Pick element at a page boundary
            size_t page = (lcg >> 16) % (n_elements / page_elements);
            size_t idx = page * page_elements + page_elements - 1;  // Last element on page
            if (idx >= n_elements) idx = n_elements - 1;

            memory_barrier();
            uint64_t t0 = read_counter();

            // Read from page boundary — DMP may or may not cross page
            uint64_t val = array[idx];
            size_t next = (val - base) / sizeof(uint64_t);
            if (next < n_elements) {
                sink += array[next];
                // Read from a completely different page
                lcg = lcg * 6364136223846793005ULL + 1;
                size_t other_page = (lcg >> 16) % (n_elements / page_elements);
                sink += array[other_page * page_elements];
            }

            memory_barrier();
            uint64_t t1 = read_counter();
            timings[i] = t1 - t0;
        }
        analyze("DMP Cross-page Confusion", timings, N_SAMPLES);
    }

    munmap(array, ARRAY_SIZE);
    return 0;
}
