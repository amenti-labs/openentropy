// Audio Clock PLL Jitter — Phase noise in audio subsystem oscillator
//
// The audio subsystem has its own Phase-Locked Loop (PLL) generating sample
// clocks (44.1/48 kHz). This PLL is thermally noisy — the voltage-controlled
// oscillator (VCO) has random phase excursions from:
//   - Thermal noise in VCO transistors (Johnson-Nyquist)
//   - Shot noise in charge pump current
//   - Reference oscillator crystal phase noise
//
// By rapidly querying AudioDeviceGetCurrentTime, we measure the relationship
// between the audio clock domain and the CPU clock domain. The jitter in
// this relationship reveals PLL phase noise.
//
// Unlike the existing audio_pll_jitter.c, this version uses a tighter
// measurement loop and probes both input and output devices.
//
// Build: cc -O2 -o thermal_audio_pll_jitter thermal_audio_pll_jitter.c \
//        -framework CoreAudio -framework AudioToolbox -framework CoreFoundation -lm

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <CoreAudio/CoreAudio.h>

#define N_SAMPLES 20000

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

static void analyze_device(AudioDeviceID device, const char *device_name) {
    printf("\n--- Device: %s (ID=%u) ---\n", device_name, device);

    // Get sample rate
    AudioObjectPropertyAddress addr = {
        .mSelector = kAudioDevicePropertyNominalSampleRate,
        .mScope = kAudioObjectPropertyScopeGlobal,
        .mElement = kAudioObjectPropertyElementMain,
    };
    Float64 sampleRate = 0;
    UInt32 size = sizeof(sampleRate);
    AudioObjectGetPropertyData(device, &addr, 0, NULL, &size, &sampleRate);
    printf("  Sample rate: %.0f Hz\n", sampleRate);

    // Method 1: Rapid audio device property queries — timing jitter
    // Each query crosses the audio/CPU clock domain boundary
    uint64_t query_timings[N_SAMPLES];

    addr.mSelector = kAudioDevicePropertyDeviceIsRunning;
    for (int i = 0; i < N_SAMPLES; i++) {
        UInt32 isRunning = 0;
        size = sizeof(isRunning);

        uint64_t t0 = mach_absolute_time();
        AudioObjectGetPropertyData(device, &addr, 0, NULL, &size, &isRunning);
        uint64_t t1 = mach_absolute_time();

        query_timings[i] = t1 - t0;
    }

    // Analyze raw timing
    uint8_t *qt_lsb = malloc(N_SAMPLES);
    uint8_t *qt_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        qt_lsb[i] = query_timings[i] & 0xFF;
        uint64_t t = query_timings[i];
        qt_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                     ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }

    printf("\n  === Query timing ===\n");
    analyze_entropy("LSBs", qt_lsb, N_SAMPLES);
    analyze_entropy("XOR-fold", qt_xor, N_SAMPLES);

    // Delta analysis
    uint8_t *qt_delta = malloc(N_SAMPLES - 1);
    for (int i = 0; i < N_SAMPLES - 1; i++) {
        int64_t d = (int64_t)query_timings[i+1] - (int64_t)query_timings[i];
        uint64_t ud = (uint64_t)d;
        qt_delta[i] = (ud & 0xFF) ^ ((ud >> 8) & 0xFF) ^
                       ((ud >> 16) & 0xFF) ^ ((ud >> 24) & 0xFF);
    }
    analyze_entropy("Delta XOR-fold", qt_delta, N_SAMPLES - 1);

    // Method 2: Sample rate query interleaved with mach_absolute_time
    // Looking for beat frequency between CPU and audio PLL
    printf("\n  === PLL beat detection ===\n");

    uint64_t beat_timings[N_SAMPLES];
    addr.mSelector = kAudioDevicePropertyActualSampleRate;
    for (int i = 0; i < N_SAMPLES; i++) {
        Float64 actual_rate = 0;
        size = sizeof(actual_rate);

        uint64_t t0 = mach_absolute_time();
        OSStatus st = AudioObjectGetPropertyData(device, &addr, 0, NULL, &size, &actual_rate);
        uint64_t t1 = mach_absolute_time();

        if (st != noErr) {
            // Fallback to nominal rate query
            addr.mSelector = kAudioDevicePropertyNominalSampleRate;
            AudioObjectGetPropertyData(device, &addr, 0, NULL, &size, &actual_rate);
            addr.mSelector = kAudioDevicePropertyActualSampleRate;
        }
        beat_timings[i] = t1 - t0;
    }

    uint8_t *bt_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t = beat_timings[i];
        bt_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                     ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("Beat timing XOR-fold", bt_xor, N_SAMPLES);

    // Method 3: Latency query — involves audio clock computation
    printf("\n  === Audio latency query timing ===\n");
    uint64_t lat_timings[N_SAMPLES];
    addr.mSelector = kAudioDevicePropertyLatency;
    addr.mScope = kAudioDevicePropertyScopeOutput;

    for (int i = 0; i < N_SAMPLES; i++) {
        UInt32 latency = 0;
        size = sizeof(latency);

        uint64_t t0 = mach_absolute_time();
        AudioObjectGetPropertyData(device, &addr, 0, NULL, &size, &latency);
        uint64_t t1 = mach_absolute_time();

        lat_timings[i] = t1 - t0;
    }

    uint8_t *lt_xor = malloc(N_SAMPLES);
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t = lat_timings[i];
        lt_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                     ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    analyze_entropy("Latency timing XOR-fold", lt_xor, N_SAMPLES);

    // Timing stats
    uint64_t tmin = UINT64_MAX, tmax = 0, tsum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        if (query_timings[i] < tmin) tmin = query_timings[i];
        if (query_timings[i] > tmax) tmax = query_timings[i];
        tsum += query_timings[i];
    }

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);
    printf("\n  Query timing stats:\n");
    printf("    Min: %llu ticks (%llu ns)\n", tmin, tmin * tb.numer / tb.denom);
    printf("    Max: %llu ticks (%llu ns)\n", tmax, tmax * tb.numer / tb.denom);
    printf("    Mean: %.1f ticks\n", (double)tsum / N_SAMPLES);

    free(qt_lsb);
    free(qt_xor);
    free(qt_delta);
    free(bt_xor);
    free(lt_xor);
}

int main(void) {
    printf("# Audio Clock PLL Jitter — Phase Noise Entropy\n\n");

    // Get default output device
    AudioObjectPropertyAddress addr = {
        .mSelector = kAudioHardwarePropertyDefaultOutputDevice,
        .mScope = kAudioObjectPropertyScopeGlobal,
        .mElement = kAudioObjectPropertyElementMain,
    };

    AudioDeviceID outDevice = 0;
    UInt32 size = sizeof(outDevice);
    OSStatus status = AudioObjectGetPropertyData(
        kAudioObjectSystemObject, &addr, 0, NULL, &size, &outDevice);

    if (status == noErr && outDevice != 0) {
        // Get device name
        addr.mSelector = kAudioObjectPropertyName;
        CFStringRef name = NULL;
        size = sizeof(name);
        AudioObjectGetPropertyData(outDevice, &addr, 0, NULL, &size, &name);
        char nameBuf[256] = "Unknown";
        if (name) {
            CFStringGetCString(name, nameBuf, sizeof(nameBuf), kCFStringEncodingUTF8);
            CFRelease(name);
        }
        analyze_device(outDevice, nameBuf);
    }

    // Get default input device
    addr.mSelector = kAudioHardwarePropertyDefaultInputDevice;
    addr.mScope = kAudioObjectPropertyScopeGlobal;
    AudioDeviceID inDevice = 0;
    size = sizeof(inDevice);
    status = AudioObjectGetPropertyData(
        kAudioObjectSystemObject, &addr, 0, NULL, &size, &inDevice);

    if (status == noErr && inDevice != 0 && inDevice != outDevice) {
        addr.mSelector = kAudioObjectPropertyName;
        CFStringRef name = NULL;
        size = sizeof(name);
        AudioObjectGetPropertyData(inDevice, &addr, 0, NULL, &size, &name);
        char nameBuf[256] = "Unknown";
        if (name) {
            CFStringGetCString(name, nameBuf, sizeof(nameBuf), kCFStringEncodingUTF8);
            CFRelease(name);
        }
        analyze_device(inDevice, nameBuf);
    }

    return 0;
}
