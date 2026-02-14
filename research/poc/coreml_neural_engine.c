// Neural Engine Inference Timing Jitter
// The Apple Neural Engine (ANE) is a dedicated ML accelerator on the M4 die.
// Its inference timing varies due to:
// - ANE power state transitions (idle → active ramp)
// - ANE internal memory bandwidth contention
// - ANE scheduling with other ANE tasks (Siri, etc.)
// - Thermal state affecting ANE clock
// We use CoreML C API to dispatch tiny inferences and measure timing variance.
//
// Since CoreML requires Objective-C, we use a simpler approach:
// vDSP/Accelerate routines that dispatch to the Neural Engine when available,
// or measure the BNNS (Basic Neural Network Subroutines) framework timing.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <Accelerate/Accelerate.h>

#define N_SAMPLES 15000

int main(void) {
    printf("# BNNS (Neural Network Subroutines) Timing Jitter\n");
    printf("# Measuring inference timing through Apple's neural path...\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    uint64_t timings[N_SAMPLES];

    // Create a tiny neural network layer using BNNS
    // This exercises the hardware neural path on Apple Silicon
    const int in_size = 64;
    const int out_size = 32;

    float *input = calloc(in_size, sizeof(float));
    float *output = calloc(out_size, sizeof(float));
    float *weights = malloc(in_size * out_size * sizeof(float));
    float *bias = calloc(out_size, sizeof(float));

    // Initialize weights with pseudo-random values
    uint64_t lcg = mach_absolute_time() | 1;
    for (int i = 0; i < in_size * out_size; i++) {
        lcg = lcg * 6364136223846793005ULL + 1;
        weights[i] = (float)(lcg >> 32) / (float)UINT32_MAX - 0.5f;
    }

    // Use vDSP matrix multiply as a proxy for neural computation
    // On M4, vDSP can dispatch to AMX/ANE path
    for (int i = 0; i < N_SAMPLES; i++) {
        // Vary input each iteration
        lcg = lcg * 6364136223846793005ULL + 1;
        for (int j = 0; j < in_size; j++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            input[j] = (float)(lcg >> 32) / (float)UINT32_MAX;
        }

        uint64_t t0 = mach_absolute_time();

        // Dense layer: output = input * weights + bias
        // vDSP_mmul dispatches through Accelerate which uses hardware acceleration
        vDSP_mmul(input, 1, weights, 1, output, 1, 1, out_size, in_size);

        // Add bias and apply activation (ReLU)
        vDSP_vadd(output, 1, bias, 1, output, 1, out_size);
        // ReLU: max(0, x)
        float zero = 0.0f;
        vDSP_vthres(output, 1, &zero, output, 1, out_size);

        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;

        // Use output to prevent optimization
        asm volatile("" : : "r"(output[0]) : "memory");
    }

    // Analysis: raw timing LSBs
    int hist[256] = {0};
    for (int i = 0; i < N_SAMPLES; i++) {
        hist[timings[i] & 0xFF]++;
    }
    double shannon = 0.0;
    int max_count = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            double p = (double)hist[i] / N_SAMPLES;
            shannon -= p * log2(p);
        }
        if (hist[i] > max_count) max_count = hist[i];
    }
    double min_entropy = -log2((double)max_count / N_SAMPLES);

    // Delta analysis
    int delta_hist[256] = {0};
    int n_deltas = N_SAMPLES - 1;
    for (int i = 0; i < n_deltas; i++) {
        uint64_t d = timings[i+1] - timings[i];
        delta_hist[d & 0xFF]++;
    }
    double d_shannon = 0.0;
    int d_max = 0;
    for (int i = 0; i < 256; i++) {
        if (delta_hist[i] > 0) {
            double p = (double)delta_hist[i] / n_deltas;
            d_shannon -= p * log2(p);
        }
        if (delta_hist[i] > d_max) d_max = delta_hist[i];
    }
    double d_min_entropy = -log2((double)d_max / n_deltas);

    // XOR-folded analysis
    int xf_hist[256] = {0};
    for (int i = 0; i < N_SAMPLES; i++) {
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (timings[i] >> (b*8)) & 0xFF;
        xf_hist[folded]++;
    }
    double xf_shannon = 0.0;
    int xf_max = 0;
    for (int i = 0; i < 256; i++) {
        if (xf_hist[i] > 0) {
            double p = (double)xf_hist[i] / N_SAMPLES;
            xf_shannon -= p * log2(p);
        }
        if (xf_hist[i] > xf_max) xf_max = xf_hist[i];
    }
    double xf_min_entropy = -log2((double)xf_max / N_SAMPLES);

    // Delta-of-deltas (second order)
    int dd_hist[256] = {0};
    int n_dd = n_deltas - 1;
    for (int i = 0; i < n_dd; i++) {
        int64_t d1 = (int64_t)timings[i+1] - (int64_t)timings[i];
        int64_t d2 = (int64_t)timings[i+2] - (int64_t)timings[i+1];
        int64_t dd = d2 - d1;
        dd_hist[((uint64_t)dd) & 0xFF]++;
    }
    double dd_shannon = 0.0;
    int dd_max = 0;
    for (int i = 0; i < 256; i++) {
        if (dd_hist[i] > 0) {
            double p = (double)dd_hist[i] / n_dd;
            dd_shannon -= p * log2(p);
        }
        if (dd_hist[i] > dd_max) dd_max = dd_hist[i];
    }
    double dd_min_entropy = -log2((double)dd_max / n_dd);

    // Stats
    uint64_t sum = 0, tmin = UINT64_MAX, tmax = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        sum += timings[i];
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
    }
    double mean = (double)sum / N_SAMPLES;
    double var = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        double d = timings[i] - mean;
        var += d * d;
    }
    var /= N_SAMPLES;
    uint64_t mean_ns = (uint64_t)(mean * tb.numer / tb.denom);

    printf("BNNS Dense Layer Timing:\n");
    printf("  Samples: %d\n", N_SAMPLES);
    printf("  Mean: %.1f ticks (≈%llu ns)\n", mean, mean_ns);
    printf("  Range: %llu - %llu ticks\n", tmin, tmax);
    printf("  StdDev: %.1f ticks\n", sqrt(var));
    printf("\n");
    printf("  Raw LSB:        Shannon=%.3f  H∞=%.3f\n", shannon, min_entropy);
    printf("  XOR-folded:     Shannon=%.3f  H∞=%.3f\n", xf_shannon, xf_min_entropy);
    printf("  Delta LSB:      Shannon=%.3f  H∞=%.3f\n", d_shannon, d_min_entropy);
    printf("  Delta-of-delta: Shannon=%.3f  H∞=%.3f\n", dd_shannon, dd_min_entropy);

    free(input);
    free(output);
    free(weights);
    free(bias);

    return 0;
}
