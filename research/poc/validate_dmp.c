// DMP Confusion — Critical Validation
// Tests:
// 1. 100K samples, full entropy analysis
// 2. Autocorrelation (lag-1 through lag-10)
// 3. Stability across 10 independent trials of 10K each
// 4. Comparison with plain cache-miss timing (is DMP the actual source?)
// 5. Comparison with sequential (DMP-predictable) vs random (DMP-confusing)

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <sys/mman.h>

#define LARGE_N 100000
#define TRIAL_N 10000
#define N_TRIALS 10
#define ARRAY_SIZE (16 * 1024 * 1024)

static inline uint64_t read_counter(void) {
    uint64_t val;
    __asm__ volatile("isb\nmrs %0, CNTVCT_EL0" : "=r"(val));
    return val;
}

static inline void memory_barrier(void) {
    __asm__ volatile("dmb sy" ::: "memory");
}

typedef struct {
    double shannon;
    double min_entropy;
    double mean;
    double stddev;
} Stats;

static Stats compute_stats(uint64_t *timings, int n) {
    Stats s = {0};
    // XOR-fold each timing to a byte
    int hist[256] = {0};
    uint64_t sum = 0;
    for (int i = 0; i < n; i++) {
        uint8_t f = 0;
        for (int b = 0; b < 8; b++) f ^= (timings[i] >> (b*8)) & 0xFF;
        hist[f]++;
        sum += timings[i];
    }
    s.mean = (double)sum / n;
    double var = 0;
    for (int i = 0; i < n; i++) {
        double d = timings[i] - s.mean;
        var += d * d;
    }
    s.stddev = sqrt(var / n);

    int mx = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            double p = (double)hist[i] / n;
            s.shannon -= p * log2(p);
        }
        if (hist[i] > mx) mx = hist[i];
    }
    s.min_entropy = -log2((double)mx / n);
    return s;
}

static Stats compute_stats_delta_xorfold(uint64_t *timings, int n) {
    Stats s = {0};
    int nd = n - 1;
    int hist[256] = {0};
    for (int i = 0; i < nd; i++) {
        int64_t d = (int64_t)timings[i+1] - (int64_t)timings[i];
        uint8_t f = 0;
        for (int b = 0; b < 8; b++) f ^= (((uint64_t)d) >> (b*8)) & 0xFF;
        hist[f]++;
    }
    int mx = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            double p = (double)hist[i] / nd;
            s.shannon -= p * log2(p);
        }
        if (hist[i] > mx) mx = hist[i];
    }
    s.min_entropy = -log2((double)mx / nd);
    return s;
}

// Autocorrelation at a given lag
static double autocorrelation(uint64_t *timings, int n, int lag) {
    if (n <= lag) return 0;
    double mean = 0;
    for (int i = 0; i < n; i++) mean += timings[i];
    mean /= n;

    double num = 0, den = 0;
    for (int i = 0; i < n - lag; i++) {
        num += (timings[i] - mean) * (timings[i + lag] - mean);
    }
    for (int i = 0; i < n; i++) {
        den += (timings[i] - mean) * (timings[i] - mean);
    }
    if (den < 1e-15) return 0;
    return num / den;
}

// Pearson correlation between two equal-length series
static double pearson(uint64_t *a, uint64_t *b, int n) {
    double ma = 0, mb = 0;
    for (int i = 0; i < n; i++) { ma += a[i]; mb += b[i]; }
    ma /= n; mb /= n;

    double num = 0, da = 0, db = 0;
    for (int i = 0; i < n; i++) {
        double x = a[i] - ma, y = b[i] - mb;
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if (da < 1e-15 || db < 1e-15) return 0;
    return num / sqrt(da * db);
}

static void collect_dmp_confusion(uint64_t *array, size_t n_elements, uint64_t base,
                                   uint64_t *timings, int n, uint64_t *lcg_state) {
    uint64_t lcg = *lcg_state;
    volatile uint64_t sink = 0;

    for (int i = 0; i < n; i++) {
        lcg = lcg * 6364136223846793005ULL + 1;
        size_t idx = (lcg >> 16) % (n_elements - 256);

        memory_barrier();
        uint64_t t0 = read_counter();

        // Triple-hop pointer chase with reversal
        uint64_t val = array[idx];
        size_t next = (val - base) / sizeof(uint64_t);
        if (next < n_elements) {
            uint64_t val2 = array[next];
            size_t next2 = (val2 - base) / sizeof(uint64_t);
            if (next2 < n_elements) {
                sink += array[next2];
                sink += array[idx > 64 ? idx - 64 : 0];
            }
        }

        memory_barrier();
        uint64_t t1 = read_counter();
        timings[i] = t1 - t0;
    }
    *lcg_state = lcg;
    (void)sink;
}

// Plain random cache-miss timing (NO pointer-like values, NO DMP trigger)
static void collect_plain_cache_miss(uint64_t *array, size_t n_elements,
                                      uint64_t *timings, int n, uint64_t *lcg_state) {
    uint64_t lcg = *lcg_state;
    volatile uint64_t sink = 0;

    for (int i = 0; i < n; i++) {
        lcg = lcg * 6364136223846793005ULL + 1;
        size_t idx1 = (lcg >> 16) % n_elements;
        lcg = lcg * 6364136223846793005ULL + 1;
        size_t idx2 = (lcg >> 16) % n_elements;

        memory_barrier();
        uint64_t t0 = read_counter();
        sink += array[idx1];
        sink += array[idx2];
        memory_barrier();
        uint64_t t1 = read_counter();
        timings[i] = t1 - t0;
    }
    *lcg_state = lcg;
    (void)sink;
}

// Sequential (DMP-predictable) access
static void collect_sequential(uint64_t *array, size_t n_elements, uint64_t base,
                                uint64_t *timings, int n) {
    volatile uint64_t sink = 0;

    for (int i = 0; i < n; i++) {
        size_t idx = ((size_t)i * 64) % (n_elements - 4);

        memory_barrier();
        uint64_t t0 = read_counter();

        // Sequential chase — DMP should predict correctly
        uint64_t val = array[idx];
        size_t next = (val - base) / sizeof(uint64_t);
        if (next < n_elements) sink += array[next];
        sink += array[(idx + 1) % n_elements];

        memory_barrier();
        uint64_t t1 = read_counter();
        timings[i] = t1 - t0;
    }
    (void)sink;
}

int main(void) {
    printf("# DMP Confusion — Critical Validation\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // Allocate pointer-filled array
    uint64_t *array = mmap(NULL, ARRAY_SIZE, PROT_READ | PROT_WRITE,
                           MAP_PRIVATE | MAP_ANON, -1, 0);
    if (array == MAP_FAILED) { perror("mmap"); return 1; }

    uint64_t base = (uint64_t)array;
    size_t n_elements = ARRAY_SIZE / sizeof(uint64_t);
    uint64_t lcg = mach_absolute_time() | 1;

    for (size_t i = 0; i < n_elements; i++) {
        lcg = lcg * 6364136223846793005ULL + 1;
        size_t offset = (lcg >> 16) % n_elements;
        array[i] = base + offset * sizeof(uint64_t);
    }

    // === TEST 1: 100K sample entropy ===
    printf("=== Test 1: 100K Sample Entropy ===\n");
    {
        uint64_t *timings = malloc(LARGE_N * sizeof(uint64_t));
        collect_dmp_confusion(array, n_elements, base, timings, LARGE_N, &lcg);

        Stats raw = compute_stats(timings, LARGE_N);
        Stats delta = compute_stats_delta_xorfold(timings, LARGE_N);

        printf("  XOR-fold:       Shannon=%.3f  H∞=%.3f\n", raw.shannon, raw.min_entropy);
        printf("  Delta XOR-fold: Shannon=%.3f  H∞=%.3f\n", delta.shannon, delta.min_entropy);
        printf("  Mean=%.1f ticks  StdDev=%.1f\n\n", raw.mean, raw.stddev);
        free(timings);
    }

    // === TEST 2: Autocorrelation ===
    printf("=== Test 2: Autocorrelation (lag 1-10) ===\n");
    {
        uint64_t *timings = malloc(LARGE_N * sizeof(uint64_t));
        collect_dmp_confusion(array, n_elements, base, timings, LARGE_N, &lcg);

        printf("  (Values near 0 = good. >0.1 or <-0.1 = concerning)\n");
        for (int lag = 1; lag <= 10; lag++) {
            double ac = autocorrelation(timings, LARGE_N, lag);
            printf("  lag-%d: %.4f %s\n", lag, ac, fabs(ac) > 0.1 ? " *** HIGH ***" : "");
        }
        printf("\n");
        free(timings);
    }

    // === TEST 3: Stability across 10 trials ===
    printf("=== Test 3: Stability (10 trials × 10K samples) ===\n");
    {
        double min_ents[N_TRIALS];
        double shannon_ents[N_TRIALS];
        uint64_t *timings = malloc(TRIAL_N * sizeof(uint64_t));

        for (int t = 0; t < N_TRIALS; t++) {
            collect_dmp_confusion(array, n_elements, base, timings, TRIAL_N, &lcg);
            Stats s = compute_stats(timings, TRIAL_N);
            min_ents[t] = s.min_entropy;
            shannon_ents[t] = s.shannon;
            printf("  Trial %2d: Shannon=%.3f  H∞=%.3f  Mean=%.0f  StdDev=%.0f\n",
                   t+1, s.shannon, s.min_entropy, s.mean, s.stddev);
        }

        double me_mean = 0, me_min = 999, me_max = 0;
        for (int i = 0; i < N_TRIALS; i++) {
            me_mean += min_ents[i];
            if (min_ents[i] < me_min) me_min = min_ents[i];
            if (min_ents[i] > me_max) me_max = min_ents[i];
        }
        me_mean /= N_TRIALS;
        double me_var = 0;
        for (int i = 0; i < N_TRIALS; i++) {
            double d = min_ents[i] - me_mean;
            me_var += d * d;
        }
        printf("\n  H∞ across trials: Mean=%.3f  Min=%.3f  Max=%.3f  StdDev=%.3f\n",
               me_mean, me_min, me_max, sqrt(me_var / N_TRIALS));
        printf("  Verdict: %s\n\n",
               (me_max - me_min) < 1.0 ? "STABLE" : "UNSTABLE — wide variation");
        free(timings);
    }

    // === TEST 4: DMP vs plain cache miss ===
    printf("=== Test 4: DMP Confusion vs Plain Cache Miss (is DMP the actual source?) ===\n");
    {
        // Create a non-pointer array (values that DON'T trigger DMP)
        uint64_t *data_array = mmap(NULL, ARRAY_SIZE, PROT_READ | PROT_WRITE,
                                     MAP_PRIVATE | MAP_ANON, -1, 0);
        for (size_t i = 0; i < n_elements; i++) {
            // Small values that don't look like pointers
            data_array[i] = (uint64_t)i * 3 + 7;
        }

        uint64_t *dmp_timings = malloc(TRIAL_N * sizeof(uint64_t));
        uint64_t *cache_timings = malloc(TRIAL_N * sizeof(uint64_t));

        collect_dmp_confusion(array, n_elements, base, dmp_timings, TRIAL_N, &lcg);
        collect_plain_cache_miss(data_array, n_elements, cache_timings, TRIAL_N, &lcg);

        Stats dmp_s = compute_stats(dmp_timings, TRIAL_N);
        Stats cache_s = compute_stats(cache_timings, TRIAL_N);
        Stats dmp_d = compute_stats_delta_xorfold(dmp_timings, TRIAL_N);
        Stats cache_d = compute_stats_delta_xorfold(cache_timings, TRIAL_N);

        printf("  DMP confusion (pointer values):\n");
        printf("    XOR-fold: Shannon=%.3f  H∞=%.3f  Mean=%.0f  StdDev=%.0f\n",
               dmp_s.shannon, dmp_s.min_entropy, dmp_s.mean, dmp_s.stddev);
        printf("    Delta:    Shannon=%.3f  H∞=%.3f\n", dmp_d.shannon, dmp_d.min_entropy);

        printf("  Plain cache miss (non-pointer values):\n");
        printf("    XOR-fold: Shannon=%.3f  H∞=%.3f  Mean=%.0f  StdDev=%.0f\n",
               cache_s.shannon, cache_s.min_entropy, cache_s.mean, cache_s.stddev);
        printf("    Delta:    Shannon=%.3f  H∞=%.3f\n", cache_d.shannon, cache_d.min_entropy);

        double diff = dmp_s.min_entropy - cache_s.min_entropy;
        printf("\n  H∞ difference (DMP - cache): %.3f bits\n", diff);
        printf("  Verdict: %s\n\n",
               diff > 0.5 ? "DMP adds significant entropy BEYOND cache noise" :
               diff > 0.1 ? "DMP adds some entropy beyond cache noise" :
               "DMP entropy is MOSTLY just cache noise — CONCERN");

        // Correlation between the two
        double r = pearson(dmp_timings, cache_timings, TRIAL_N);
        printf("  Pearson correlation (DMP vs cache miss): %.4f\n", r);
        printf("  Verdict: %s\n\n", fabs(r) < 0.1 ? "Independent" :
               fabs(r) < 0.3 ? "Weakly correlated" : "SIGNIFICANTLY correlated — CONCERN");

        free(dmp_timings);
        free(cache_timings);
        munmap(data_array, ARRAY_SIZE);
    }

    // === TEST 5: Sequential (DMP-predictable) vs random ===
    printf("=== Test 5: Sequential (DMP-predictable) vs Random (DMP-confusing) ===\n");
    {
        uint64_t *seq_timings = malloc(TRIAL_N * sizeof(uint64_t));
        uint64_t *rnd_timings = malloc(TRIAL_N * sizeof(uint64_t));

        collect_sequential(array, n_elements, base, seq_timings, TRIAL_N);
        collect_dmp_confusion(array, n_elements, base, rnd_timings, TRIAL_N, &lcg);

        Stats seq = compute_stats(seq_timings, TRIAL_N);
        Stats rnd = compute_stats(rnd_timings, TRIAL_N);

        printf("  Sequential (DMP succeeds): Shannon=%.3f  H∞=%.3f  Mean=%.0f\n",
               seq.shannon, seq.min_entropy, seq.mean);
        printf("  Random (DMP confused):     Shannon=%.3f  H∞=%.3f  Mean=%.0f\n",
               rnd.shannon, rnd.min_entropy, rnd.mean);
        printf("  H∞ difference: %.3f  Mean timing difference: %.0f ticks\n",
               rnd.min_entropy - seq.min_entropy, rnd.mean - seq.mean);
        printf("  Verdict: %s\n\n",
               (rnd.min_entropy - seq.min_entropy) > 0.3 ?
               "DMP confusion MEASURABLY increases entropy vs predictable access" :
               "Minimal difference — DMP may not be the entropy driver — CONCERN");

        free(seq_timings);
        free(rnd_timings);
    }

    munmap(array, ARRAY_SIZE);
    return 0;
}
