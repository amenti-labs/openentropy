// Neural Engine (ANE) Inference Jitter — Independent processor timing entropy
//
// Apple's Neural Engine is a completely separate processor with its own clock,
// memory controller, and DVFS. By running inference repeatedly and timing each,
// we capture entropy from an entirely independent domain.
//
// We use CoreML via the coremltools-generated prediction API. Since C doesn't
// have direct CoreML bindings easily, we use the Obj-C runtime to call CoreML.
//
// Alternative approach: Use the ANE indirectly through Accelerate/BNNS which
// may get dispatched to the ANE for certain operations.
//
// Build: cc -O2 -o unprecedented_ane_jitter unprecedented_ane_jitter.c -framework Accelerate -framework CoreFoundation -lm
// (BNNS approach — doesn't need CoreML model file)

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <Accelerate/Accelerate.h>

#define N_SAMPLES 12000
#define MATRIX_SIZE 64   // Small enough to be fast, large enough for real work
#define INNER_OPS 4      // Multiple operations per timing sample

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

// Volatile sink to prevent dead-code elimination
static volatile float v_sink = 0.0f;

int main(void) {
    printf("# Neural Engine / Accelerate Framework Inference Jitter\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === Approach 1: BLAS matrix operations (may hit AMX/ANE) ===
    printf("--- Test 1: BLAS sgemm Timing ---\n");

    int M = MATRIX_SIZE, N = MATRIX_SIZE, K = MATRIX_SIZE;
    float *A = malloc(M * K * sizeof(float));
    float *B = malloc(K * N * sizeof(float));
    float *C = malloc(M * N * sizeof(float));

    // Initialize with deterministic but non-trivial values
    for (int i = 0; i < M * K; i++) A[i] = sinf((float)i * 0.01f);
    for (int i = 0; i < K * N; i++) B[i] = cosf((float)i * 0.01f);

    uint64_t *timings = malloc(N_SAMPLES * sizeof(uint64_t));
    uint8_t *lsbs = malloc(N_SAMPLES);

    for (int i = 0; i < N_SAMPLES; i++) {
        memset(C, 0, M * N * sizeof(float));

        uint64_t t0 = mach_absolute_time();
        for (int j = 0; j < INNER_OPS; j++) {
            cblas_sgemm(CblasRowMajor, CblasNoTrans, CblasNoTrans,
                       M, N, K, 1.0f, A, K, B, N, 0.0f, C, N);
        }
        uint64_t t1 = mach_absolute_time();

        v_sink = C[0]; // Prevent optimization
        timings[i] = t1 - t0;
        lsbs[i] = (uint8_t)(timings[i] & 0xFF);
    }

    uint64_t tmin = timings[0], tmax = timings[0], tsum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
        tsum += timings[i];
    }
    printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
    analyze_entropy("BLAS sgemm LSBs", lsbs, N_SAMPLES);

    // === Test 2: Delta timing ===
    printf("\n--- Test 2: BLAS sgemm Delta Timing ---\n");
    uint8_t *deltas = malloc(N_SAMPLES);
    for (int i = 1; i < N_SAMPLES; i++) {
        int64_t d = (int64_t)timings[i] - (int64_t)timings[i-1];
        uint64_t ud = (uint64_t)d;
        deltas[i-1] = (uint8_t)((ud >> 0) ^ (ud >> 8) ^ (ud >> 16) ^ (ud >> 24));
    }
    analyze_entropy("BLAS delta XOR-fold", deltas, N_SAMPLES - 1);

    // === Approach 2: vDSP FFT operations ===
    printf("\n--- Test 3: vDSP FFT Timing ---\n");

    int fft_n = 1024;
    int log2n = 10;
    FFTSetup fft_setup = vDSP_create_fftsetup(log2n, FFT_RADIX2);

    float *fft_real = malloc(fft_n * sizeof(float));
    float *fft_imag = malloc(fft_n * sizeof(float));
    DSPSplitComplex split = { .realp = fft_real, .imagp = fft_imag };

    for (int i = 0; i < fft_n; i++) {
        fft_real[i] = sinf((float)i * 0.1f) + cosf((float)i * 0.07f);
        fft_imag[i] = 0.0f;
    }

    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        for (int j = 0; j < INNER_OPS; j++) {
            vDSP_fft_zip(fft_setup, &split, 1, log2n, FFT_FORWARD);
            vDSP_fft_zip(fft_setup, &split, 1, log2n, FFT_INVERSE);
        }
        uint64_t t1 = mach_absolute_time();
        v_sink = fft_real[0];
        timings[i] = t1 - t0;
        lsbs[i] = (uint8_t)(timings[i] & 0xFF);
    }

    tmin = timings[0]; tmax = timings[0]; tsum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
        tsum += timings[i];
    }
    printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
    analyze_entropy("vDSP FFT LSBs", lsbs, N_SAMPLES);

    // === Test 4: vDSP FFT Delta ===
    printf("\n--- Test 4: vDSP FFT Delta ---\n");
    for (int i = 1; i < N_SAMPLES; i++) {
        int64_t d = (int64_t)timings[i] - (int64_t)timings[i-1];
        uint64_t ud = (uint64_t)d;
        deltas[i-1] = (uint8_t)((ud >> 0) ^ (ud >> 8) ^ (ud >> 16) ^ (ud >> 24));
    }
    analyze_entropy("vDSP FFT delta XOR-fold", deltas, N_SAMPLES - 1);

    // === Approach 3: BNNS (neural network) operations ===
    printf("\n--- Test 5: BNNS Neural Network Inference Timing ---\n");

    // Create a simple fully-connected layer via BNNS
    float *nn_input = calloc(MATRIX_SIZE, sizeof(float));
    float *nn_output = calloc(MATRIX_SIZE, sizeof(float));
    float *nn_weights = malloc(MATRIX_SIZE * MATRIX_SIZE * sizeof(float));
    float *nn_bias = calloc(MATRIX_SIZE, sizeof(float));

    for (int i = 0; i < MATRIX_SIZE * MATRIX_SIZE; i++)
        nn_weights[i] = sinf((float)i * 0.001f) * 0.1f;
    for (int i = 0; i < MATRIX_SIZE; i++)
        nn_input[i] = cosf((float)i * 0.05f);

    BNNSLayerParametersFullyConnected fc_params = {
        .i_desc = {
            .layout = BNNSDataLayoutVector,
            .size = { MATRIX_SIZE, 0, 0 },
            .data_type = BNNSDataTypeFloat32,
        },
        .o_desc = {
            .layout = BNNSDataLayoutVector,
            .size = { MATRIX_SIZE, 0, 0 },
            .data_type = BNNSDataTypeFloat32,
        },
        .w_desc = {
            .layout = BNNSDataLayoutRowMajorMatrix,
            .size = { MATRIX_SIZE, MATRIX_SIZE, 0 },
            .data_type = BNNSDataTypeFloat32,
            .data = nn_weights,
        },
        .bias = {
            .data_type = BNNSDataTypeFloat32,
            .data = nn_bias,
        },
        .activation = { .function = BNNSActivationFunctionRectifiedLinear },
    };

    BNNSFilter nn_filter = BNNSFilterCreateLayerFullyConnected(
        &fc_params, NULL);

    if (nn_filter) {
        for (int i = 0; i < N_SAMPLES; i++) {
            // Slightly vary input each iteration
            nn_input[i % MATRIX_SIZE] += 0.001f;

            uint64_t t0 = mach_absolute_time();
            for (int j = 0; j < INNER_OPS; j++) {
                BNNSFilterApply(nn_filter, nn_input, nn_output);
            }
            uint64_t t1 = mach_absolute_time();
            v_sink = nn_output[0];
            timings[i] = t1 - t0;
            lsbs[i] = (uint8_t)(timings[i] & 0xFF);
        }

        tmin = timings[0]; tmax = timings[0]; tsum = 0;
        for (int i = 0; i < N_SAMPLES; i++) {
            if (timings[i] < tmin) tmin = timings[i];
            if (timings[i] > tmax) tmax = timings[i];
            tsum += timings[i];
        }
        printf("  Timing range: %llu - %llu ticks, mean=%llu\n", tmin, tmax, tsum/N_SAMPLES);
        analyze_entropy("BNNS inference LSBs", lsbs, N_SAMPLES);

        printf("\n--- Test 6: BNNS Delta ---\n");
        for (int i = 1; i < N_SAMPLES; i++) {
            int64_t d = (int64_t)timings[i] - (int64_t)timings[i-1];
            uint64_t ud = (uint64_t)d;
            deltas[i-1] = (uint8_t)((ud >> 0) ^ (ud >> 8) ^ (ud >> 16) ^ (ud >> 24));
        }
        analyze_entropy("BNNS delta XOR-fold", deltas, N_SAMPLES - 1);

        BNNSFilterDestroy(nn_filter);
    } else {
        printf("  BNNS filter creation failed\n");
    }

    free(A); free(B); free(C);
    free(timings); free(lsbs); free(deltas);
    free(fft_real); free(fft_imag);
    free(nn_input); free(nn_output); free(nn_weights); free(nn_bias);
    vDSP_destroy_fftsetup(fft_setup);

    printf("\nDone.\n");
    return 0;
}
