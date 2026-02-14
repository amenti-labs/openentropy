// IOKit Deep Sensor Sweep
// Dumps ALL rapidly-changing numeric properties from the IORegistry.
// Goes beyond what ioreg -l shows — uses IORegistryEntryCreateCFProperties
// on every IOService to find hidden sensor ADC values, counters, and
// other rapidly-changing numeric values that might contain entropy.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <IOKit/IOKitLib.h>
#include <CoreFoundation/CoreFoundation.h>

#define N_READS 100  // Read each property this many times to detect changes

typedef struct {
    char class_name[256];
    char prop_name[256];
    int64_t values[N_READS];
    int n_values;
    int n_unique;
    double shannon;
    double min_entropy;
} PropertyResult;

static int n_results = 0;
static PropertyResult results[10000];

static void analyze_property(const char *class_name, const char *prop_name,
                              int64_t *values, int n) {
    if (n < 10) return;

    // Count unique values
    int64_t unique[N_READS];
    int n_unique = 0;
    for (int i = 0; i < n; i++) {
        int found = 0;
        for (int j = 0; j < n_unique; j++) {
            if (unique[j] == values[i]) { found = 1; break; }
        }
        if (!found && n_unique < N_READS) unique[n_unique++] = values[i];
    }

    // Skip if constant
    if (n_unique <= 1) return;

    // Compute delta LSB histogram
    int hist[256] = {0};
    int n_deltas = 0;
    for (int i = 1; i < n; i++) {
        int64_t d = values[i] - values[i-1];
        hist[((uint64_t)d) & 0xFF]++;
        n_deltas++;
    }

    if (n_deltas < 10) return;

    double shannon = 0.0;
    int max_count = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) {
            double p = (double)hist[i] / n_deltas;
            shannon -= p * log2(p);
        }
        if (hist[i] > max_count) max_count = hist[i];
    }
    double min_entropy = -log2((double)max_count / n_deltas);

    if (n_results < 10000) {
        PropertyResult *r = &results[n_results++];
        strncpy(r->class_name, class_name, 255);
        strncpy(r->prop_name, prop_name, 255);
        memcpy(r->values, values, n * sizeof(int64_t));
        r->n_values = n;
        r->n_unique = n_unique;
        r->shannon = shannon;
        r->min_entropy = min_entropy;
    }
}

static void scan_entry(io_registry_entry_t entry) {
    // Get class name
    io_name_t className;
    IOObjectGetClass(entry, className);

    // Get all properties
    CFMutableDictionaryRef props = NULL;
    if (IORegistryEntryCreateCFProperties(entry, &props, kCFAllocatorDefault, 0) != KERN_SUCCESS || !props) {
        return;
    }

    // Iterate all properties looking for numeric values
    CFIndex count = CFDictionaryGetCount(props);
    if (count > 0 && count < 1000) {
        const void **keys = malloc(count * sizeof(void*));
        const void **vals = malloc(count * sizeof(void*));
        CFDictionaryGetKeysAndValues(props, keys, vals);

        for (CFIndex i = 0; i < count; i++) {
            if (CFGetTypeID(keys[i]) != CFStringGetTypeID()) continue;

            char propName[256];
            CFStringGetCString((CFStringRef)keys[i], propName, sizeof(propName), kCFStringEncodingUTF8);

            // Check if value is numeric
            if (CFGetTypeID(vals[i]) == CFNumberGetTypeID()) {
                int64_t val;
                CFNumberGetValue((CFNumberRef)vals[i], kCFNumberSInt64Type, &val);

                // Read this property multiple times rapidly
                int64_t readings[N_READS];
                readings[0] = val;
                int n_read = 1;

                for (int r = 1; r < N_READS; r++) {
                    CFMutableDictionaryRef p2 = NULL;
                    if (IORegistryEntryCreateCFProperties(entry, &p2, kCFAllocatorDefault, 0) == KERN_SUCCESS && p2) {
                        CFNumberRef num = (CFNumberRef)CFDictionaryGetValue(p2, keys[i]);
                        if (num && CFGetTypeID(num) == CFNumberGetTypeID()) {
                            CFNumberGetValue(num, kCFNumberSInt64Type, &readings[n_read++]);
                        }
                        CFRelease(p2);
                    }
                }

                analyze_property(className, propName, readings, n_read);
            }
            // Also check CFData for raw sensor bytes
            else if (CFGetTypeID(vals[i]) == CFDataGetTypeID()) {
                CFDataRef data = (CFDataRef)vals[i];
                CFIndex len = CFDataGetLength(data);
                if (len >= 4 && len <= 64) {
                    // Read as raw bytes, treat first 8 bytes as uint64
                    const uint8_t *bytes = CFDataGetBytePtr(data);
                    int64_t readings[N_READS];
                    readings[0] = 0;
                    for (int b = 0; b < len && b < 8; b++) {
                        readings[0] |= ((int64_t)bytes[b]) << (b * 8);
                    }
                    int n_read = 1;

                    for (int r = 1; r < N_READS; r++) {
                        CFMutableDictionaryRef p2 = NULL;
                        if (IORegistryEntryCreateCFProperties(entry, &p2, kCFAllocatorDefault, 0) == KERN_SUCCESS && p2) {
                            CFDataRef d2 = (CFDataRef)CFDictionaryGetValue(p2, keys[i]);
                            if (d2 && CFGetTypeID(d2) == CFDataGetTypeID() && CFDataGetLength(d2) == len) {
                                const uint8_t *b2 = CFDataGetBytePtr(d2);
                                readings[n_read] = 0;
                                for (int b = 0; b < len && b < 8; b++) {
                                    readings[n_read] |= ((int64_t)b2[b]) << (b * 8);
                                }
                                n_read++;
                            }
                            CFRelease(p2);
                        }
                    }

                    char dataName[300];
                    snprintf(dataName, sizeof(dataName), "%s[data:%ldb]", propName, (long)len);
                    analyze_property(className, dataName, readings, n_read);
                }
            }
        }

        free(keys);
        free(vals);
    }

    CFRelease(props);
}

int main(void) {
    printf("# IOKit Deep Sensor Sweep\n");
    printf("# Scanning ALL IORegistry entries for rapidly-changing numeric values...\n\n");

    // Iterate all IOService entries in the registry
    io_iterator_t iter;
    kern_return_t kr = IOServiceGetMatchingServices(
        kIOMainPortDefault, IOServiceMatching("IOService"), &iter);

    if (kr != KERN_SUCCESS) {
        fprintf(stderr, "Failed to get IOService iterator: %d\n", kr);
        return 1;
    }

    int n_scanned = 0;
    io_registry_entry_t entry;
    while ((entry = IOIteratorNext(iter)) != 0) {
        scan_entry(entry);
        IOObjectRelease(entry);
        n_scanned++;
    }
    IOObjectRelease(iter);

    // Also scan the IOPower plane
    kr = IOServiceGetMatchingServices(
        kIOMainPortDefault, IOServiceMatching("AppleSMCKeysEndpoint"), &iter);
    if (kr == KERN_SUCCESS) {
        while ((entry = IOIteratorNext(iter)) != 0) {
            scan_entry(entry);
            IOObjectRelease(entry);
            n_scanned++;
        }
        IOObjectRelease(iter);
    }

    printf("Scanned %d IORegistry entries, found %d changing properties\n\n", n_scanned, n_results);

    // Sort by min_entropy descending
    for (int i = 0; i < n_results - 1; i++) {
        for (int j = i + 1; j < n_results; j++) {
            if (results[j].min_entropy > results[i].min_entropy) {
                PropertyResult tmp = results[i];
                results[i] = results[j];
                results[j] = tmp;
            }
        }
    }

    // Print top results
    printf("Top changing properties (by H∞):\n");
    printf("%-30s %-40s %8s %8s %8s\n", "Class", "Property", "Unique", "Shannon", "H∞");
    printf("%-30s %-40s %8s %8s %8s\n", "-----", "--------", "------", "-------", "--");
    for (int i = 0; i < n_results && i < 50; i++) {
        PropertyResult *r = &results[i];
        printf("%-30s %-40s %8d %8.3f %8.3f\n",
               r->class_name, r->prop_name, r->n_unique, r->shannon, r->min_entropy);
    }

    return 0;
}
