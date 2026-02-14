// Audio ADC Noise Floor — Johnson-Nyquist thermal noise
//
// Captures raw audio from the microphone via CoreAudio with input muted/silent.
// The LSBs of each 32-bit float sample are dominated by:
//   - Johnson-Nyquist thermal noise in the input impedance (V² = 4kTRΔf)
//   - Shot noise in the ADC comparator transistors
//   - Quantization noise from the sigma-delta modulator
//
// Unlike the ffmpeg-based audio_noise source, this uses CoreAudio directly
// for lower latency and access to raw float samples without resampling.
//
// Build: cc -O2 -o thermal_audio_adc_noise thermal_audio_adc_noise.c \
//        -framework CoreAudio -framework AudioToolbox -framework CoreFoundation -lm

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <unistd.h>
#include <mach/mach_time.h>
#include <CoreAudio/CoreAudio.h>
#include <AudioToolbox/AudioToolbox.h>

#define N_SAMPLES 20000
#define BUFFER_SIZE 4096

// Ring buffer for captured audio
static float g_audio_buffer[N_SAMPLES * 2];
static volatile int g_samples_collected = 0;

// AudioQueue callback — copies raw float samples into our buffer
static void audio_input_callback(
    void *inUserData,
    AudioQueueRef inAQ,
    AudioQueueBufferRef inBuffer,
    const AudioTimeStamp *inStartTime,
    UInt32 inNumPackets,
    const AudioStreamPacketDescription *inPacketDesc)
{
    float *samples = (float *)inBuffer->mAudioData;
    int n = inBuffer->mAudioDataByteSize / sizeof(float);

    for (int i = 0; i < n && g_samples_collected < N_SAMPLES * 2; i++) {
        g_audio_buffer[g_samples_collected++] = samples[i];
    }

    // Re-enqueue the buffer for continuous capture
    AudioQueueEnqueueBuffer(inAQ, inBuffer, 0, NULL);
}

// Entropy analysis helper
static void analyze_bytes(const char *label, const uint8_t *data, int n) {
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
    printf("# Audio ADC Noise Floor — Johnson-Nyquist Thermal Noise\n");
    printf("# Capturing raw audio from microphone with CoreAudio...\n\n");

    // Set up audio format: 32-bit float, mono, 44100 Hz
    AudioStreamBasicDescription format = {0};
    format.mSampleRate = 44100.0;
    format.mFormatID = kAudioFormatLinearPCM;
    format.mFormatFlags = kLinearPCMFormatFlagIsFloat | kLinearPCMFormatFlagIsPacked;
    format.mBitsPerChannel = 32;
    format.mChannelsPerFrame = 1;
    format.mBytesPerFrame = 4;
    format.mFramesPerPacket = 1;
    format.mBytesPerPacket = 4;

    AudioQueueRef queue = NULL;
    OSStatus status = AudioQueueNewInput(&format, audio_input_callback,
                                          NULL, NULL, NULL, 0, &queue);
    if (status != noErr) {
        fprintf(stderr, "AudioQueueNewInput failed: %d\n", (int)status);
        return 1;
    }

    // Allocate and enqueue buffers
    for (int i = 0; i < 3; i++) {
        AudioQueueBufferRef buf;
        status = AudioQueueAllocateBuffer(queue, BUFFER_SIZE * sizeof(float), &buf);
        if (status != noErr) {
            fprintf(stderr, "AudioQueueAllocateBuffer failed: %d\n", (int)status);
            return 1;
        }
        AudioQueueEnqueueBuffer(queue, buf, 0, NULL);
    }

    // Start recording
    status = AudioQueueStart(queue, NULL);
    if (status != noErr) {
        fprintf(stderr, "AudioQueueStart failed: %d\n", (int)status);
        return 1;
    }

    printf("Recording... (collecting %d float samples)\n", N_SAMPLES);

    // Wait for samples
    int timeout_ms = 5000;
    while (g_samples_collected < N_SAMPLES && timeout_ms > 0) {
        usleep(10000); // 10ms
        timeout_ms -= 10;
    }

    AudioQueueStop(queue, true);
    AudioQueueDispose(queue, true);

    int n = g_samples_collected;
    if (n < 100) {
        fprintf(stderr, "Only collected %d samples, need at least 100\n", n);
        return 1;
    }
    printf("Collected %d float samples\n\n", n);

    // Method 1: Raw float LSBs
    // Reinterpret float bits, extract lower 8 bits (mantissa LSBs)
    uint8_t *float_lsbs = malloc(n);
    uint8_t *float_lsb4 = malloc(n);
    for (int i = 0; i < n; i++) {
        uint32_t bits;
        memcpy(&bits, &g_audio_buffer[i], sizeof(bits));
        float_lsbs[i] = bits & 0xFF;
        float_lsb4[i] = bits & 0x0F;
    }

    printf("=== Method 1: Float mantissa LSBs ===\n");
    analyze_bytes("Raw byte-0 (mantissa LSBs)", float_lsbs, n);

    // Pack nibbles for 4-bit analysis
    int n_packed = n / 2;
    uint8_t *packed_nibbles = malloc(n_packed);
    for (int i = 0; i < n_packed; i++) {
        packed_nibbles[i] = (float_lsb4[i*2] << 4) | float_lsb4[i*2+1];
    }
    analyze_bytes("Packed 4-bit nibbles", packed_nibbles, n_packed);

    // Method 2: Sample magnitude — quiet mic means values near zero
    // The absolute value's distribution tells us about noise floor
    printf("\n=== Method 2: Sample magnitude analysis ===\n");
    double sum_sq = 0.0;
    double max_val = 0.0;
    for (int i = 0; i < n; i++) {
        double v = fabs(g_audio_buffer[i]);
        sum_sq += v * v;
        if (v > max_val) max_val = v;
    }
    double rms = sqrt(sum_sq / n);
    printf("  RMS level: %.8f (%.1f dBFS)\n", rms, 20.0 * log10(rms + 1e-30));
    printf("  Peak level: %.8f (%.1f dBFS)\n", max_val, 20.0 * log10(max_val + 1e-30));

    // Method 3: Delta of consecutive samples
    printf("\n=== Method 3: Consecutive sample deltas ===\n");
    int nd = n - 1;
    uint8_t *delta_lsbs = malloc(nd);
    uint8_t *delta_xor = malloc(nd);
    for (int i = 0; i < nd; i++) {
        uint32_t bits_a, bits_b;
        memcpy(&bits_a, &g_audio_buffer[i], sizeof(bits_a));
        memcpy(&bits_b, &g_audio_buffer[i+1], sizeof(bits_b));
        int32_t delta = (int32_t)bits_b - (int32_t)bits_a;
        delta_lsbs[i] = (uint8_t)(delta & 0xFF);
        // XOR-fold for wider coverage
        uint32_t ud = (uint32_t)delta;
        delta_xor[i] = (ud & 0xFF) ^ ((ud >> 8) & 0xFF) ^
                        ((ud >> 16) & 0xFF) ^ ((ud >> 24) & 0xFF);
    }
    analyze_bytes("Delta LSBs", delta_lsbs, nd);
    analyze_bytes("Delta XOR-folded", delta_xor, nd);

    // Method 4: Interleaved timing — time between AudioQueue callbacks
    printf("\n=== Method 4: Sample statistics ===\n");
    printf("  First 20 raw float values:\n    ");
    for (int i = 0; i < 20 && i < n; i++) {
        printf("%.8f ", g_audio_buffer[i]);
    }
    printf("\n");
    printf("  First 20 mantissa LSBs:\n    ");
    for (int i = 0; i < 20 && i < n; i++) {
        printf("%02x ", float_lsbs[i]);
    }
    printf("\n");

    // Autocorrelation of LSBs (lag 1-5)
    printf("\n=== Autocorrelation (mantissa LSBs) ===\n");
    double mean = 0;
    for (int i = 0; i < n; i++) mean += float_lsbs[i];
    mean /= n;
    double var = 0;
    for (int i = 0; i < n; i++) {
        double d = float_lsbs[i] - mean;
        var += d * d;
    }
    var /= n;
    for (int lag = 1; lag <= 5; lag++) {
        double cov = 0;
        for (int i = 0; i < n - lag; i++) {
            cov += (float_lsbs[i] - mean) * (float_lsbs[i+lag] - mean);
        }
        cov /= (n - lag);
        printf("  Lag %d: r=%.4f\n", lag, cov / var);
    }

    free(float_lsbs);
    free(float_lsb4);
    free(packed_nibbles);
    free(delta_lsbs);
    free(delta_xor);

    return 0;
}
