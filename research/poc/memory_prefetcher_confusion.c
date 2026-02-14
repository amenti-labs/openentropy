// Memory Prefetcher Confusion Timing
// Apple's DMP (Data Memory-dependent Prefetcher) is unique to Apple Silicon.
// It reads memory VALUES (not just addresses) to predict future accesses.
// By creating access patterns that confuse the DMP, we measure prediction
// failure timing — a genuinely novel entropy source.
//
// The DMP was only discovered/published in 2023 (GoFetch paper).
// Nobody has used its prediction failures as an entropy source.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <sys/mman.h>

#define N_SAMPLES 20000
#define ARRAY_SIZE (16 * 1024 * 1024)  // 16MB — larger than SLC

static inline uint64_t read_counter(void) {
    uint64_t val;
    __asm__ volatile("isb\nmrs %0, CNTVCT_EL0" : "=r"(val));
    return val;
}

static inline void memory_barrier(void) {
    __asm__ volatile("dmb sy" ::: "memory");
}

int main(void) {
    printf("# Memory Prefetcher (DMP) Confusion Timing\n");
    printf("# Exploiting Apple's Data Memory-dependent Prefetcher prediction failures\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // Allocate a large array that looks like pointers to the DMP
    // The DMP tries to dereference VALUES it sees in memory
    uint64_t *array = mmap(NULL, ARRAY_SIZE, PROT_READ | PROT_WRITE,
                           MAP_PRIVATE | MAP_ANON, -1, 0);
    if (array == MAP_FAILED) {
        perror("mmap");
        return 1;
    }

    // Fill with values that look like valid pointers but point to different cache lines
    // This maximizes DMP confusion — it will try to prefetch these "pointers"
    uint64_t base = (uint64_t)array;
    size_t n_elements = ARRAY_SIZE / sizeof(uint64_t);
    uint64_t lcg = mach_absolute_time() | 1;

    for (size_t i = 0; i < n_elements; i++) {
        lcg = lcg * 6364136223846793005ULL + 1;
        // Create values that look like pointers within the array
        // but with enough randomness to confuse the DMP
        size_t random_offset = (lcg >> 16) % n_elements;
        array[i] = base + random_offset * sizeof(uint64_t);
    }

    // ===== TEST 1: DMP confusion — chase "pointers" in a confused pattern =====
    printf("=== Test 1: DMP Pointer-Chase Confusion ===\n");
    {
        uint64_t timings[N_SAMPLES];
        volatile uint64_t sink = 0;

        for (int i = 0; i < N_SAMPLES; i++) {
            // Start from a pseudo-random position
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t idx = (lcg >> 16) % (n_elements - 256);

            memory_barrier();
            uint64_t t0 = read_counter();

            // Chase the "pointer" — the DMP will try to prefetch the target
            // but we immediately change direction, confusing it
            uint64_t val = array[idx];
            // Compute next index from the loaded value — this is what DMP predicts
            size_t next = (val - base) / sizeof(uint64_t);
            if (next < n_elements) {
                // Load from the predicted location — DMP may have prefetched this
                sink += array[next];
            }
            // Now load from an UNPREDICTABLE location — DMP gets it wrong
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t surprise = (lcg >> 16) % n_elements;
            sink += array[surprise];

            memory_barrier();
            uint64_t t1 = read_counter();
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
        printf("  XOR-folded: Shannon=%.3f  H∞=%.3f\n", xs, -log2((double)xm/N_SAMPLES));

        // Delta
        int dh[256] = {0};
        int nd = N_SAMPLES - 1;
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
        printf("  Delta XOR-fold: Shannon=%.3f  H∞=%.3f\n\n", ds, -log2((double)dm/nd));
    }

    // ===== TEST 2: Cache-level boundary probing =====
    // Access at specific offsets to hit L1/L2/SLC/DRAM boundaries
    // M4: L1=192KB, L2=16MB(shared), SLC=~36MB
    printf("=== Test 2: Cache Boundary Transition Timing ===\n");
    {
        // Different strides to probe different cache levels
        size_t strides[] = {
            64,          // Within cache line
            4096,        // L1 page boundary
            192 * 1024,  // ~L1 size
            4 * 1024 * 1024,  // L2 region
            12 * 1024 * 1024, // SLC boundary
        };
        const char *names[] = {"cache_line", "page", "L1_boundary", "L2_region", "SLC_boundary"};

        for (int s = 0; s < 5; s++) {
            uint64_t timings[N_SAMPLES];
            volatile uint64_t sink = 0;
            size_t stride = strides[s];

            for (int i = 0; i < N_SAMPLES; i++) {
                size_t idx = ((size_t)i * stride / sizeof(uint64_t)) % n_elements;

                memory_barrier();
                uint64_t t0 = read_counter();
                sink += array[idx];
                memory_barrier();
                uint64_t t1 = read_counter();
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
            printf("  %s (stride=%zu): XOR-fold Shannon=%.3f  H∞=%.3f\n",
                   names[s], stride, sh, -log2((double)mx/N_SAMPLES));
        }
        printf("\n");
    }

    // ===== TEST 3: Interleaved DMP + non-DMP patterns =====
    // Alternate between "pointer-like" and "data-like" access patterns
    // to maximally confuse the DMP's heuristic
    printf("=== Test 3: DMP Mode-Switching Confusion ===\n");
    {
        uint64_t timings[N_SAMPLES];
        volatile uint64_t sink = 0;

        // Create a second array with non-pointer values (floats cast to uint64)
        uint64_t *data_array = mmap(NULL, 1024 * 1024, PROT_READ | PROT_WRITE,
                                     MAP_PRIVATE | MAP_ANON, -1, 0);
        size_t data_len = 1024 * 1024 / sizeof(uint64_t);
        for (size_t i = 0; i < data_len; i++) {
            // Values that DON'T look like pointers (small values, float patterns)
            double f = (double)i * 3.14159;
            memcpy(&data_array[i], &f, sizeof(uint64_t));
        }

        for (int i = 0; i < N_SAMPLES; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;

            memory_barrier();
            uint64_t t0 = read_counter();

            if (i & 1) {
                // Pointer-like pattern — DMP activates
                size_t idx = (lcg >> 16) % (n_elements - 1);
                uint64_t val = array[idx];
                size_t next = (val - base) / sizeof(uint64_t);
                if (next < n_elements) sink += array[next];
            } else {
                // Data-like pattern — DMP should NOT activate
                size_t idx = (lcg >> 16) % data_len;
                sink += data_array[idx];
                sink += data_array[(idx + 1) % data_len];
            }

            memory_barrier();
            uint64_t t1 = read_counter();
            timings[i] = t1 - t0;
        }

        // Analyze odd (pointer) vs even (data) separately and combined
        int hist_all[256] = {0}, hist_ptr[256] = {0}, hist_data[256] = {0};
        int n_ptr = 0, n_data = 0;
        for (int i = 0; i < N_SAMPLES; i++) {
            uint8_t f = 0;
            for (int b = 0; b < 8; b++) f ^= (timings[i] >> (b*8)) & 0xFF;
            hist_all[f]++;
            if (i & 1) { hist_ptr[f]++; n_ptr++; }
            else { hist_data[f]++; n_data++; }
        }

        for (int t = 0; t < 3; t++) {
            int *h = t == 0 ? hist_all : (t == 1 ? hist_ptr : hist_data);
            int n = t == 0 ? N_SAMPLES : (t == 1 ? n_ptr : n_data);
            const char *label = t == 0 ? "Combined" : (t == 1 ? "Pointer-mode" : "Data-mode");
            double sh = 0; int mx = 0;
            for (int i = 0; i < 256; i++) {
                if (h[i] > 0) { double p = (double)h[i]/n; sh -= p*log2(p); }
                if (h[i] > mx) mx = h[i];
            }
            printf("  %s: XOR-fold Shannon=%.3f  H∞=%.3f\n", label, sh, -log2((double)mx/n));
        }

        munmap(data_array, 1024 * 1024);
    }

    munmap(array, ARRAY_SIZE);
    return 0;
}
