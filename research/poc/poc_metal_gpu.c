/*
 * PoC 5: GPU Command Buffer Submission Timing (Metal API via IOKit)
 *
 * Since we can't easily use Metal from C, this PoC tests a different approach:
 * Using IOKit to observe GPU-related timing that varies with GPU state.
 *
 * Physics: The M4's GPU shares the unified memory controller and SLC with
 * the CPU. GPU command buffer submission goes through:
 * 1. IOKit submission to AGXAccelerator
 * 2. GPU command stream parsing
 * 3. Memory management unit (DART/IOMMU) translation
 * 4. SLC arbitration between GPU and CPU
 *
 * Instead of Metal, we measure IOKit GPU-related operations and
 * IOServiceGetMatchingService timing which traverses the IORegistry
 * and reflects current driver state.
 *
 * ALSO: We test Accelerate framework vDSP operations which dispatch to
 * the GPU on M-series when beneficial.
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <mach/mach_time.h>
#include <Accelerate/Accelerate.h>
#include <IOKit/IOKitLib.h>
#include <math.h>

#define NUM_SAMPLES 5000

int main() {
    printf("=== GPU/Accelerate Framework Timing Entropy ===\n\n");

    uint64_t timings[NUM_SAMPLES];

    // Method 1: vDSP FFT timing — dispatches to AMX/GPU depending on size
    printf("--- Method 1: vDSP FFT timing (various sizes) ---\n");

    int fft_sizes[] = {64, 128, 256, 512, 1024, 2048, 4096};
    int n_fft_sizes = sizeof(fft_sizes) / sizeof(fft_sizes[0]);
    uint64_t lcg = mach_absolute_time() | 1;

    for (int s = 0; s < NUM_SAMPLES; s++) {
        int n = fft_sizes[s % n_fft_sizes];
        int log2n = 0;
        int tmp = n;
        while (tmp > 1) { tmp >>= 1; log2n++; }

        // Allocate and fill with pseudo-random data
        float *real = (float *)calloc(n, sizeof(float));
        float *imag = (float *)calloc(n, sizeof(float));
        for (int i = 0; i < n; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            real[i] = (float)(lcg >> 32) / (float)UINT32_MAX;
            lcg = lcg * 6364136223846793005ULL + 1;
            imag[i] = (float)(lcg >> 32) / (float)UINT32_MAX;
        }

        DSPSplitComplex split = {real, imag};
        FFTSetup fft = vDSP_create_fftsetup(log2n, FFT_RADIX2);

        uint64_t t0 = mach_absolute_time();
        vDSP_fft_zip(fft, &split, 1, log2n, FFT_FORWARD);
        uint64_t t1 = mach_absolute_time();

        timings[s] = t1 - t0;

        vDSP_destroy_fftsetup(fft);
        free(real);
        free(imag);
    }

    // Analyze
    int hist1[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(timings[s] >> (b * 8));
        hist1[folded]++;
    }
    double sh1 = 0.0; int u1 = 0, m1 = 0;
    for (int i = 0; i < 256; i++) {
        if (hist1[i] > 0) {
            u1++;
            if (hist1[i] > m1) m1 = hist1[i];
            double p = (double)hist1[i] / NUM_SAMPLES;
            sh1 -= p * log2(p);
        }
    }
    printf("FFT timing: unique=%d Shannon=%.3f Min-H∞=%.3f\n",
           u1, sh1, -log2((double)m1 / NUM_SAMPLES));

    // Method 2: vDSP convolution timing (exercises different data paths)
    printf("\n--- Method 2: vDSP convolution timing ---\n");

    for (int s = 0; s < NUM_SAMPLES; s++) {
        int sizes[] = {256, 512, 1024, 2048};
        int n = sizes[s % 4];
        int filter_len = 16 + (s % 48);  // Variable filter length

        float *signal = (float *)calloc(n, sizeof(float));
        float *filter = (float *)calloc(filter_len, sizeof(float));
        float *result = (float *)calloc(n + filter_len - 1, sizeof(float));

        for (int i = 0; i < n; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            signal[i] = (float)(lcg >> 32) / (float)UINT32_MAX;
        }
        for (int i = 0; i < filter_len; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            filter[i] = (float)(lcg >> 32) / (float)UINT32_MAX;
        }

        uint64_t t0 = mach_absolute_time();
        vDSP_conv(signal, 1, filter, 1, result, 1, n, filter_len);
        uint64_t t1 = mach_absolute_time();

        timings[s] = t1 - t0;
        __asm__ volatile("" ::: "memory"); // Prevent optimization

        free(signal);
        free(filter);
        free(result);
    }

    int hist2[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(timings[s] >> (b * 8));
        hist2[folded]++;
    }
    double sh2 = 0.0; int u2 = 0, m2 = 0;
    for (int i = 0; i < 256; i++) {
        if (hist2[i] > 0) {
            u2++;
            if (hist2[i] > m2) m2 = hist2[i];
            double p = (double)hist2[i] / NUM_SAMPLES;
            sh2 -= p * log2(p);
        }
    }
    printf("Convolution timing: unique=%d Shannon=%.3f Min-H∞=%.3f\n",
           u2, sh2, -log2((double)m2 / NUM_SAMPLES));

    // Method 3: IOKit GPU service lookup timing
    printf("\n--- Method 3: IOKit AGX service timing ---\n");

    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint64_t t0 = mach_absolute_time();

        // Look up GPU service — traverses IORegistry tree
        io_service_t service = IOServiceGetMatchingService(
            kIOMainPortDefault,
            IOServiceMatching("AGXAcceleratorG15P")  // M4 GPU class
        );
        if (service == 0) {
            // Try generic matching
            service = IOServiceGetMatchingService(
                kIOMainPortDefault,
                IOServiceMatching("IOAccelerator")
            );
        }

        uint64_t t1 = mach_absolute_time();

        if (service) {
            IOObjectRelease(service);
        }

        timings[s] = t1 - t0;
    }

    int hist3[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(timings[s] >> (b * 8));
        hist3[folded]++;
    }
    double sh3 = 0.0; int u3 = 0, m3 = 0;
    for (int i = 0; i < 256; i++) {
        if (hist3[i] > 0) {
            u3++;
            if (hist3[i] > m3) m3 = hist3[i];
            double p = (double)hist3[i] / NUM_SAMPLES;
            sh3 -= p * log2(p);
        }
    }
    printf("IOKit AGX lookup: unique=%d Shannon=%.3f Min-H∞=%.3f\n",
           u3, sh3, -log2((double)m3 / NUM_SAMPLES));

    // Method 4: BNNS (Basic Neural Network Subroutines) timing
    // This exercises the ANE (Apple Neural Engine) or AMX depending on operation
    printf("\n--- Method 4: vDSP large matrix operations ---\n");

    for (int s = 0; s < NUM_SAMPLES; s++) {
        int n = 64 + (s % 5) * 32; // 64, 96, 128, 160, 192
        int len = n * n;
        float *a = (float *)calloc(len, sizeof(float));
        float *b = (float *)calloc(len, sizeof(float));
        float *c = (float *)calloc(len, sizeof(float));

        for (int i = 0; i < len; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            a[i] = (float)(lcg >> 32) / (float)UINT32_MAX;
            lcg = lcg * 6364136223846793005ULL + 1;
            b[i] = (float)(lcg >> 32) / (float)UINT32_MAX;
        }

        uint64_t t0 = mach_absolute_time();
        // Matrix multiply via Accelerate (dispatches to AMX on Apple Silicon)
        vDSP_mmul(a, 1, b, 1, c, 1, n, n, n);
        uint64_t t1 = mach_absolute_time();

        timings[s] = t1 - t0;
        __asm__ volatile("" ::: "memory");

        free(a);
        free(b);
        free(c);
    }

    int hist4[256] = {0};
    for (int s = 0; s < NUM_SAMPLES; s++) {
        uint8_t folded = 0;
        for (int b = 0; b < 8; b++) folded ^= (uint8_t)(timings[s] >> (b * 8));
        hist4[folded]++;
    }
    double sh4 = 0.0; int u4 = 0, m4 = 0;
    for (int i = 0; i < 256; i++) {
        if (hist4[i] > 0) {
            u4++;
            if (hist4[i] > m4) m4 = hist4[i];
            double p = (double)hist4[i] / NUM_SAMPLES;
            sh4 -= p * log2(p);
        }
    }
    printf("vDSP mmul timing: unique=%d Shannon=%.3f Min-H∞=%.3f\n",
           u4, sh4, -log2((double)m4 / NUM_SAMPLES));

    return 0;
}
