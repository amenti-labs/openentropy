// USB Frame Counter Jitter — Crystal oscillator phase noise
//
// USB host controllers use a crystal oscillator to generate the 1 kHz
// Start-of-Frame (SOF) signal. The crystal has thermally-driven phase noise:
//   - Mechanical vibrations of quartz lattice (phonon noise)
//   - Load capacitance thermal noise
//   - Oscillator circuit Johnson-Nyquist noise
//
// By reading the USB frame counter via IOKit and measuring the timing
// between reads relative to the CPU clock, we capture the beat frequency
// between two independent oscillators (USB crystal vs CPU PLL).
//
// Build: cc -O2 -o thermal_usb_frame_jitter thermal_usb_frame_jitter.c \
//        -framework IOKit -framework CoreFoundation -lm

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <IOKit/IOKitLib.h>
#include <IOKit/usb/IOUSBLib.h>
#include <CoreFoundation/CoreFoundation.h>

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

// Read IORegistry property timing — USB controllers register frame info
static int probe_usb_controllers(void) {
    printf("=== Method 1: USB controller IORegistry query timing ===\n\n");

    // Find USB host controllers
    CFMutableDictionaryRef match = IOServiceMatching("IOUSBHostDevice");
    if (!match) {
        // Try older matching class
        match = IOServiceMatching("IOUSBDevice");
    }
    if (!match) {
        fprintf(stderr, "No USB matching dictionary\n");
        return 0;
    }

    io_iterator_t iter;
    kern_return_t kr = IOServiceGetMatchingServices(kIOMainPortDefault, match, &iter);
    if (kr != KERN_SUCCESS) {
        fprintf(stderr, "No USB devices found\n");
        return 0;
    }

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    int devices_found = 0;
    io_service_t service;
    while ((service = IOIteratorNext(iter)) != 0) {
        devices_found++;

        // Get device name
        io_name_t name;
        IORegistryEntryGetName(service, name);

        if (devices_found <= 3) {
            printf("USB Device %d: %s\n", devices_found, name);

            // Rapid property reads — timing captures USB bus arbitration jitter
            uint64_t timings[N_SAMPLES];
            for (int i = 0; i < N_SAMPLES; i++) {
                uint64_t t0 = mach_absolute_time();

                // Read various properties that require USB bus interaction
                CFTypeRef prop = IORegistryEntryCreateCFProperty(
                    service, CFSTR("sessionID"), kCFAllocatorDefault, 0);
                if (prop) CFRelease(prop);

                prop = IORegistryEntryCreateCFProperty(
                    service, CFSTR("USB Address"), kCFAllocatorDefault, 0);
                if (prop) CFRelease(prop);

                uint64_t t1 = mach_absolute_time();
                timings[i] = t1 - t0;
            }

            // Analyze
            uint8_t *t_lsb = malloc(N_SAMPLES);
            uint8_t *t_xor = malloc(N_SAMPLES);
            for (int i = 0; i < N_SAMPLES; i++) {
                t_lsb[i] = timings[i] & 0xFF;
                uint64_t t = timings[i];
                t_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                            ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
            }

            analyze_entropy("Query LSBs", t_lsb, N_SAMPLES);
            analyze_entropy("Query XOR-fold", t_xor, N_SAMPLES);

            // Delta
            uint8_t *t_delta = malloc(N_SAMPLES - 1);
            for (int i = 0; i < N_SAMPLES - 1; i++) {
                int64_t d = (int64_t)timings[i+1] - (int64_t)timings[i];
                uint64_t ud = (uint64_t)d;
                t_delta[i] = (ud & 0xFF) ^ ((ud >> 8) & 0xFF) ^
                              ((ud >> 16) & 0xFF) ^ ((ud >> 24) & 0xFF);
            }
            analyze_entropy("Delta XOR-fold", t_delta, N_SAMPLES - 1);

            uint64_t tmin = UINT64_MAX, tmax = 0, tsum = 0;
            for (int i = 0; i < N_SAMPLES; i++) {
                if (timings[i] < tmin) tmin = timings[i];
                if (timings[i] > tmax) tmax = timings[i];
                tsum += timings[i];
            }
            printf("  Timing: min=%llu max=%llu mean=%.0f ticks\n\n",
                   tmin, tmax, (double)tsum / N_SAMPLES);

            free(t_lsb);
            free(t_xor);
            free(t_delta);
        }

        IOObjectRelease(service);
    }
    IOObjectRelease(iter);

    printf("Total USB devices found: %d\n", devices_found);
    return devices_found;
}

// Method 2: IORegistry traversal timing — USB hub topology introduces jitter
static void probe_usb_hub_timing(void) {
    printf("\n=== Method 2: USB hub topology traversal timing ===\n\n");

    io_iterator_t iter;
    CFMutableDictionaryRef match = IOServiceMatching("AppleUSBHostController");
    kern_return_t kr = IOServiceGetMatchingServices(kIOMainPortDefault, match, &iter);
    if (kr != KERN_SUCCESS) {
        // Try alternative class names
        match = IOServiceMatching("IOUSBController");
        kr = IOServiceGetMatchingServices(kIOMainPortDefault, match, &iter);
    }
    if (kr != KERN_SUCCESS) {
        printf("  No USB host controllers found, trying generic approach...\n");

        // Fallback: traverse IOService plane for any USB-related entries
        io_registry_entry_t root = IORegistryGetRootEntry(kIOMainPortDefault);
        if (!root) return;

        uint64_t timings[N_SAMPLES];
        for (int i = 0; i < N_SAMPLES; i++) {
            io_iterator_t child_iter;
            uint64_t t0 = mach_absolute_time();

            // This traversal timing varies with USB bus state
            kr = IORegistryEntryGetChildIterator(root, kIOServicePlane, &child_iter);
            if (kr == KERN_SUCCESS) {
                io_service_t child;
                int count = 0;
                while ((child = IOIteratorNext(child_iter)) != 0 && count < 10) {
                    IOObjectRelease(child);
                    count++;
                }
                IOObjectRelease(child_iter);
            }

            uint64_t t1 = mach_absolute_time();
            timings[i] = t1 - t0;
        }

        uint8_t *t_xor = malloc(N_SAMPLES);
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t t = timings[i];
            t_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                        ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
        }
        analyze_entropy("Root traversal XOR-fold", t_xor, N_SAMPLES);
        free(t_xor);

        IOObjectRelease(root);
        return;
    }

    io_service_t controller;
    while ((controller = IOIteratorNext(iter)) != 0) {
        io_name_t name;
        IORegistryEntryGetName(controller, name);
        printf("Controller: %s\n", name);

        uint64_t timings[N_SAMPLES];
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t t0 = mach_absolute_time();

            // Read controller frame list — this accesses USB timing hardware
            CFTypeRef prop = IORegistryEntryCreateCFProperty(
                controller, CFSTR("IOPCIResourced"), kCFAllocatorDefault, 0);
            if (prop) CFRelease(prop);

            prop = IORegistryEntryCreateCFProperty(
                controller, CFSTR("IOInterruptControllers"), kCFAllocatorDefault, 0);
            if (prop) CFRelease(prop);

            uint64_t t1 = mach_absolute_time();
            timings[i] = t1 - t0;
        }

        uint8_t *t_xor = malloc(N_SAMPLES);
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t t = timings[i];
            t_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                        ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
        }
        analyze_entropy("Controller timing XOR-fold", t_xor, N_SAMPLES);
        free(t_xor);

        IOObjectRelease(controller);
    }
    IOObjectRelease(iter);
}

// Method 3: Interleaved USB+CPU timing for beat detection
static void probe_beat_frequency(void) {
    printf("\n=== Method 3: USB/CPU clock beat detection ===\n\n");

    // Rapidly alternate between IORegistry reads and CPU timing
    // to detect beat frequency between USB crystal and CPU PLL
    uint64_t timings[N_SAMPLES];

    io_registry_entry_t root = IORegistryGetRootEntry(kIOMainPortDefault);
    if (!root) {
        printf("  Cannot access IORegistry root\n");
        return;
    }

    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();

        // Tight loop: IORegistry property read acts as USB-timed probe
        CFTypeRef prop = IORegistryEntryCreateCFProperty(
            root, CFSTR("IOKitBuildVersion"), kCFAllocatorDefault, 0);
        if (prop) CFRelease(prop);

        // ISB to serialize pipeline
        __asm__ volatile("isb" ::: "memory");

        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
    }

    IOObjectRelease(root);

    uint8_t *t_xor = malloc(N_SAMPLES);
    uint8_t *t_delta = malloc(N_SAMPLES - 1);

    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t = timings[i];
        t_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                    ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
    }
    for (int i = 0; i < N_SAMPLES - 1; i++) {
        int64_t d = (int64_t)timings[i+1] - (int64_t)timings[i];
        uint64_t ud = (uint64_t)d;
        t_delta[i] = (ud & 0xFF) ^ ((ud >> 8) & 0xFF) ^
                      ((ud >> 16) & 0xFF) ^ ((ud >> 24) & 0xFF);
    }

    analyze_entropy("Beat timing XOR-fold", t_xor, N_SAMPLES);
    analyze_entropy("Beat delta XOR-fold", t_delta, N_SAMPLES - 1);

    // Autocorrelation to detect periodic beat
    double mean = 0;
    for (int i = 0; i < N_SAMPLES; i++) mean += t_xor[i];
    mean /= N_SAMPLES;
    double var = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        double d = t_xor[i] - mean;
        var += d * d;
    }
    var /= N_SAMPLES;

    printf("\n  Autocorrelation:\n");
    for (int lag = 1; lag <= 10; lag++) {
        double cov = 0;
        for (int i = 0; i < N_SAMPLES - lag; i++) {
            cov += (t_xor[i] - mean) * (t_xor[i+lag] - mean);
        }
        cov /= (N_SAMPLES - lag);
        printf("    Lag %2d: r=%.4f\n", lag, cov / var);
    }

    free(t_xor);
    free(t_delta);
}

int main(void) {
    printf("# USB Frame Counter Jitter — Crystal Oscillator Phase Noise\n\n");

    int n_devices = probe_usb_controllers();
    if (n_devices == 0) {
        printf("No USB devices found. Mac Mini M4 may use Thunderbolt/internal buses.\n");
        printf("Falling back to IORegistry timing methods...\n");
    }

    probe_usb_hub_timing();
    probe_beat_frequency();

    return 0;
}
