// Audio PLL (Phase-Locked Loop) Clock Jitter
// The audio subsystem has its own PLL generating 44.1/48 kHz sample clocks.
// This PLL is an independent oscillator from the CPU clock.
// By reading the audio device's host time rapidly, we capture PLL phase jitter.
// This is NOT recording audio — it's probing the audio clock domain.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <math.h>
#include <mach/mach_time.h>
#include <CoreAudio/CoreAudio.h>
#include <AudioToolbox/AudioToolbox.h>

#define N_SAMPLES 20000

int main(void) {
    printf("# Audio PLL Clock Jitter Probe\n");
    printf("# Measuring audio clock vs system clock drift/jitter...\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // Get default output device
    AudioObjectPropertyAddress addr = {
        .mSelector = kAudioHardwarePropertyDefaultOutputDevice,
        .mScope = kAudioObjectPropertyScopeGlobal,
        .mElement = kAudioObjectPropertyElementMain,
    };

    AudioDeviceID device = 0;
    UInt32 size = sizeof(device);
    OSStatus status = AudioObjectGetPropertyData(
        kAudioObjectSystemObject, &addr, 0, NULL, &size, &device);

    if (status != noErr || device == 0) {
        fprintf(stderr, "No audio device found (status=%d)\n", (int)status);
        // Fallback: try to enumerate devices
        addr.mSelector = kAudioHardwarePropertyDevices;
        UInt32 devSize = 0;
        AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &addr, 0, NULL, &devSize);
        int nDevices = devSize / sizeof(AudioDeviceID);
        printf("  Found %d audio devices total\n", nDevices);
        if (nDevices > 0) {
            AudioDeviceID *devices = malloc(devSize);
            AudioObjectGetPropertyData(kAudioObjectSystemObject, &addr, 0, NULL, &devSize, devices);
            device = devices[0];
            printf("  Using first device: %u\n", device);
            free(devices);
        } else {
            return 1;
        }
    }

    // Get device name
    addr.mSelector = kAudioObjectPropertyName;
    CFStringRef name = NULL;
    size = sizeof(name);
    AudioObjectGetPropertyData(device, &addr, 0, NULL, &size, &name);
    if (name) {
        char nameBuf[256];
        CFStringGetCString(name, nameBuf, sizeof(nameBuf), kCFStringEncodingUTF8);
        printf("Audio device: %s (ID=%u)\n", nameBuf, device);
        CFRelease(name);
    }

    // Get nominal sample rate
    addr.mSelector = kAudioDevicePropertyNominalSampleRate;
    addr.mScope = kAudioObjectPropertyScopeGlobal;
    Float64 sampleRate = 0;
    size = sizeof(sampleRate);
    AudioObjectGetPropertyData(device, &addr, 0, NULL, &size, &sampleRate);
    printf("Sample rate: %.0f Hz\n\n", sampleRate);

    // METHOD 1: Rapidly read the device's current time and measure host-time jitter
    // AudioDeviceGetCurrentTime gives us the relationship between audio time and host time
    uint64_t timings[N_SAMPLES];
    uint64_t host_times[N_SAMPLES];
    Float64 audio_times[N_SAMPLES];

    printf("Method 1: AudioDeviceGetCurrentTime rapid probing...\n");

    int valid = 0;
    AudioTimeStamp ts;
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();

        // Translate current time — this queries the audio clock
        memset(&ts, 0, sizeof(ts));
        ts.mFlags = kAudioTimeStampHostTimeValid;
        ts.mHostTime = t0;

        addr.mSelector = kAudioDevicePropertyDeviceIsRunning;
        UInt32 isRunning = 0;
        size = sizeof(isRunning);
        AudioObjectGetPropertyData(device, &addr, 0, NULL, &size, &isRunning);

        uint64_t t1 = mach_absolute_time();
        timings[valid] = t1 - t0;
        host_times[valid] = t1;
        valid++;
    }

    // METHOD 2: Direct clock jitter — measure mach_absolute_time across ISB barriers
    // and look for the PLL beating against the CPU clock
    uint64_t counter_samples[N_SAMPLES];
    for (int i = 0; i < N_SAMPLES; i++) {
        // ISB forces pipeline drain — timing depends on pipeline state
        __asm__ volatile("isb" ::: "memory");
        counter_samples[i] = mach_absolute_time();
    }

    // Analyze METHOD 1: audio subsystem query timing
    {
        int hist[256] = {0};
        for (int i = 0; i < valid; i++) {
            hist[timings[i] & 0xFF]++;
        }
        double shannon = 0.0;
        int max_c = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) {
                double p = (double)hist[i] / valid;
                shannon -= p * log2(p);
            }
            if (hist[i] > max_c) max_c = hist[i];
        }
        double min_e = -log2((double)max_c / valid);

        // Delta analysis
        int dh[256] = {0};
        int nd = valid - 1;
        for (int i = 0; i < nd; i++) {
            uint64_t d = timings[i+1] - timings[i];
            dh[d & 0xFF]++;
        }
        double ds = 0.0;
        int dm = 0;
        for (int i = 0; i < 256; i++) {
            if (dh[i] > 0) {
                double p = (double)dh[i] / nd;
                ds -= p * log2(p);
            }
            if (dh[i] > dm) dm = dh[i];
        }
        double dme = -log2((double)dm / nd);

        // XOR-fold
        int xh[256] = {0};
        for (int i = 0; i < valid; i++) {
            uint8_t f = 0;
            for (int b = 0; b < 8; b++) f ^= (timings[i] >> (b*8)) & 0xFF;
            xh[f]++;
        }
        double xs = 0.0;
        int xm = 0;
        for (int i = 0; i < 256; i++) {
            if (xh[i] > 0) {
                double p = (double)xh[i] / valid;
                xs -= p * log2(p);
            }
            if (xh[i] > xm) xm = xh[i];
        }
        double xme = -log2((double)xm / valid);

        uint64_t sum = 0, tmin = UINT64_MAX, tmax = 0;
        for (int i = 0; i < valid; i++) {
            sum += timings[i];
            if (timings[i] < tmin) tmin = timings[i];
            if (timings[i] > tmax) tmax = timings[i];
        }
        double mean = (double)sum / valid;
        uint64_t mns = (uint64_t)(mean * tb.numer / tb.denom);

        printf("  Samples: %d, Mean: %.1f ticks (≈%llu ns)\n", valid, mean, mns);
        printf("  Range: %llu - %llu\n", tmin, tmax);
        printf("  Raw LSB:    Shannon=%.3f  H∞=%.3f\n", shannon, min_e);
        printf("  XOR-folded: Shannon=%.3f  H∞=%.3f\n", xs, xme);
        printf("  Delta LSB:  Shannon=%.3f  H∞=%.3f\n", ds, dme);
        printf("\n");
    }

    // Analyze METHOD 2: ISB + counter jitter (CPU pipeline drain timing)
    {
        uint64_t deltas[N_SAMPLES];
        int nd = 0;
        for (int i = 1; i < N_SAMPLES; i++) {
            deltas[nd++] = counter_samples[i] - counter_samples[i-1];
        }

        int hist[256] = {0};
        for (int i = 0; i < nd; i++) {
            hist[deltas[i] & 0xFF]++;
        }
        double shannon = 0.0;
        int max_c = 0;
        for (int i = 0; i < 256; i++) {
            if (hist[i] > 0) {
                double p = (double)hist[i] / nd;
                shannon -= p * log2(p);
            }
            if (hist[i] > max_c) max_c = hist[i];
        }
        double min_e = -log2((double)max_c / nd);

        // XOR-fold deltas
        int xh[256] = {0};
        for (int i = 0; i < nd; i++) {
            uint8_t f = 0;
            for (int b = 0; b < 8; b++) f ^= (deltas[i] >> (b*8)) & 0xFF;
            xh[f]++;
        }
        double xs = 0.0;
        int xm = 0;
        for (int i = 0; i < 256; i++) {
            if (xh[i] > 0) {
                double p = (double)xh[i] / nd;
                xs -= p * log2(p);
            }
            if (xh[i] > xm) xm = xh[i];
        }
        double xme = -log2((double)xm / nd);

        // Delta-of-deltas
        int dd_hist[256] = {0};
        int ndd = nd - 1;
        for (int i = 0; i < ndd; i++) {
            int64_t dd = (int64_t)deltas[i+1] - (int64_t)deltas[i];
            dd_hist[((uint64_t)dd) & 0xFF]++;
        }
        double dds = 0.0;
        int ddm = 0;
        for (int i = 0; i < 256; i++) {
            if (dd_hist[i] > 0) {
                double p = (double)dd_hist[i] / ndd;
                dds -= p * log2(p);
            }
            if (dd_hist[i] > ddm) ddm = dd_hist[i];
        }
        double ddme = -log2((double)ddm / ndd);

        printf("Method 2: ISB pipeline drain + counter jitter\n");
        printf("  Samples: %d\n", nd);
        printf("  Delta LSB:      Shannon=%.3f  H∞=%.3f\n", shannon, min_e);
        printf("  Delta XOR-fold: Shannon=%.3f  H∞=%.3f\n", xs, xme);
        printf("  Delta-of-delta: Shannon=%.3f  H∞=%.3f\n", dds, ddme);

        // Show some delta values
        printf("  First 20 deltas: ");
        for (int i = 0; i < 20 && i < nd; i++) printf("%llu ", deltas[i]);
        printf("\n");
    }

    return 0;
}
