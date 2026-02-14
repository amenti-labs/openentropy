// Floating-Point Denormal Timing — Data-dependent microcode assist
//
// When floating-point numbers fall into the "denormalized" range (between 0
// and DBL_MIN ≈ 2.2e-308), most CPUs handle them via microcode assist rather
// than the fast hardware FPU path. This creates data-dependent timing:
//
//   - Normal floats: 3-5 cycles (hardware FPU)
//   - Denormal floats: 50-200+ cycles (microcode assist trap)
//
// The exact timing depends on the specific denormal value, FPU pipeline state,
// and micro-architectural state — making it a novel entropy source.
//
// On Apple Silicon (M1-M4), denormal handling may be faster than x86 but
// still creates measurable timing variation.
//
// Build: cc -O2 -o thermal_denormal_timing thermal_denormal_timing.c -lm
// Note: Do NOT use -ffast-math (it would flush denormals to zero)

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <float.h>
#include <mach/mach_time.h>

#define N_SAMPLES 20000
#define INNER_OPS 100  // Operations per timing measurement

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

// Volatile to prevent optimization
static volatile double sink = 0.0;

int main(void) {
    printf("# Floating-Point Denormal Timing — Microcode Assist Entropy\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    printf("DBL_MIN = %e\n", DBL_MIN);
    printf("DBL_TRUE_MIN = %e (smallest denormal)\n", DBL_TRUE_MIN);
    printf("Inner ops per sample: %d\n\n", INNER_OPS);

    // Generate array of denormal values with varying mantissa patterns
    double denormals[INNER_OPS];
    double normals[INNER_OPS];
    uint64_t lcg = mach_absolute_time() | 1;

    for (int i = 0; i < INNER_OPS; i++) {
        // Create denormal by starting from DBL_TRUE_MIN and scaling
        lcg = lcg * 6364136223846793005ULL + 1;
        uint64_t mantissa = lcg;
        double d;
        // Construct denormal: exponent = 0, random mantissa
        uint64_t bits = mantissa & 0x000FFFFFFFFFFFFFULL; // mantissa only, exp=0
        memcpy(&d, &bits, sizeof(d));
        denormals[i] = d;

        // Normal values for comparison
        normals[i] = 1.0 + (double)(lcg >> 32) / (double)UINT32_MAX;
    }

    // Verify we have actual denormals
    int n_denorm = 0;
    for (int i = 0; i < INNER_OPS; i++) {
        if (fpclassify(denormals[i]) == FP_SUBNORMAL) n_denorm++;
    }
    printf("Verified denormals: %d/%d\n\n", n_denorm, INNER_OPS);

    // === Method 1: Time denormal multiply operations ===
    printf("=== Method 1: Denormal multiply timing ===\n");

    uint64_t *denorm_timings = malloc(N_SAMPLES * sizeof(uint64_t));
    uint64_t *normal_timings = malloc(N_SAMPLES * sizeof(uint64_t));

    for (int s = 0; s < N_SAMPLES; s++) {
        // Denormal operations
        double acc = denormals[s % INNER_OPS];
        uint64_t t0 = mach_absolute_time();
        for (int i = 0; i < INNER_OPS; i++) {
            acc *= denormals[i];
            acc += denormals[(i + 1) % INNER_OPS];
        }
        uint64_t t1 = mach_absolute_time();
        sink = acc; // prevent dead code elimination
        denorm_timings[s] = t1 - t0;
    }

    for (int s = 0; s < N_SAMPLES; s++) {
        // Normal operations (baseline)
        double acc = normals[s % INNER_OPS];
        uint64_t t0 = mach_absolute_time();
        for (int i = 0; i < INNER_OPS; i++) {
            acc *= normals[i];
            acc += normals[(i + 1) % INNER_OPS];
        }
        uint64_t t1 = mach_absolute_time();
        sink = acc;
        normal_timings[s] = t1 - t0;
    }

    // Analyze denormal timings
    uint8_t *d_lsb = malloc(N_SAMPLES);
    uint8_t *d_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        d_lsb[i] = denorm_timings[i] & 0xFF;
        uint64_t t = denorm_timings[i];
        d_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                    ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("Denormal LSBs", d_lsb, N_SAMPLES);
    analyze_entropy("Denormal XOR-fold", d_xor, N_SAMPLES);

    // Analyze normal timings for comparison
    uint8_t *n_lsb = malloc(N_SAMPLES);
    uint8_t *n_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        n_lsb[i] = normal_timings[i] & 0xFF;
        uint64_t t = normal_timings[i];
        n_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                    ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("Normal LSBs (baseline)", n_lsb, N_SAMPLES);
    analyze_entropy("Normal XOR-fold (baseline)", n_xor, N_SAMPLES);

    // Mean timing comparison
    uint64_t d_sum = 0, n_sum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        d_sum += denorm_timings[i];
        n_sum += normal_timings[i];
    }
    printf("  Denormal mean: %.1f ticks (%.0f ns)\n",
           (double)d_sum / N_SAMPLES,
           (double)d_sum / N_SAMPLES * tb.numer / tb.denom);
    printf("  Normal mean:   %.1f ticks (%.0f ns)\n",
           (double)n_sum / N_SAMPLES,
           (double)n_sum / N_SAMPLES * tb.numer / tb.denom);
    printf("  Slowdown ratio: %.2fx\n\n",
           (double)d_sum / n_sum);

    // === Method 2: Mixed denormal/normal alternation ===
    printf("=== Method 2: Mixed denormal/normal alternation ===\n");

    uint64_t *mixed_timings = malloc(N_SAMPLES * sizeof(uint64_t));
    for (int s = 0; s < N_SAMPLES; s++) {
        lcg = lcg * 6364136223846793005ULL + 1;
        int use_denorm = (lcg >> 32) & 1;

        double *vals = use_denorm ? denormals : normals;
        double acc = vals[s % INNER_OPS];

        uint64_t t0 = mach_absolute_time();
        for (int i = 0; i < INNER_OPS; i++) {
            acc *= vals[i];
            acc += vals[(i + 1) % INNER_OPS];
        }
        uint64_t t1 = mach_absolute_time();
        sink = acc;
        mixed_timings[s] = t1 - t0;
    }

    uint8_t *m_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t = mixed_timings[i];
        m_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                    ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("Mixed timing XOR-fold", m_xor, N_SAMPLES);

    // === Method 3: Denormal add/subtract chain with varying values ===
    printf("\n=== Method 3: Denormal add/subtract chain ===\n");

    uint64_t *chain_timings = malloc(N_SAMPLES * sizeof(uint64_t));
    for (int s = 0; s < N_SAMPLES; s++) {
        // Create fresh denormal from timing seed
        lcg = lcg * 6364136223846793005ULL + 1;
        uint64_t bits = lcg & 0x000FFFFFFFFFFFFFULL;
        double d;
        memcpy(&d, &bits, sizeof(d));

        double acc = d;
        uint64_t t0 = mach_absolute_time();
        for (int i = 0; i < INNER_OPS; i++) {
            acc = acc + denormals[i] - denormals[(i + 3) % INNER_OPS];
            acc = acc * 0.999999; // stay in denormal range
        }
        uint64_t t1 = mach_absolute_time();
        sink = acc;
        chain_timings[s] = t1 - t0;
    }

    uint8_t *c_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t = chain_timings[i];
        c_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                    ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("Chain timing XOR-fold", c_xor, N_SAMPLES);

    // Delta analysis for all methods
    printf("\n=== Delta analysis ===\n");
    uint8_t *dd_xor = malloc(N_SAMPLES - 1);
    for (int i = 0; i < N_SAMPLES - 1; i++) {
        int64_t d = (int64_t)denorm_timings[i+1] - (int64_t)denorm_timings[i];
        uint64_t ud = (uint64_t)d;
        dd_xor[i] = (ud & 0xFF) ^ ((ud >> 8) & 0xFF) ^
                     ((ud >> 16) & 0xFF) ^ ((ud >> 24) & 0xFF);
    }
    analyze_entropy("Denormal delta XOR-fold", dd_xor, N_SAMPLES - 1);

    printf("\n  First 20 denormal timings: ");
    for (int i = 0; i < 20; i++) printf("%llu ", denorm_timings[i]);
    printf("\n  First 20 normal timings:   ");
    for (int i = 0; i < 20; i++) printf("%llu ", normal_timings[i]);
    printf("\n");

    free(denorm_timings);
    free(normal_timings);
    free(mixed_timings);
    free(chain_timings);
    free(d_lsb);
    free(d_xor);
    free(n_lsb);
    free(n_xor);
    free(m_xor);
    free(c_xor);
    free(dd_xor);
    return 0;
}
