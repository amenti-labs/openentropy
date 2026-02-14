// Keychain Write Timing — Refined
// SecItemAdd showed H∞ = 6.5+, the highest of ALL tests.
// This goes through: userspace → securityd → SEP → APFS COW write → return
// Every component in the chain adds independent jitter.
// This PoC tests at higher sample counts with variations.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <Security/Security.h>
#include <CoreFoundation/CoreFoundation.h>

#define N_SAMPLES 5000

static void analyze(const char *name, uint64_t *timings, int n) {
    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    int rh[256] = {0}, xh[256] = {0}, dh[256] = {0};
    for (int i = 0; i < n; i++) {
        rh[timings[i] & 0xFF]++;
        uint8_t f = 0;
        for (int b = 0; b < 8; b++) f ^= (timings[i] >> (b*8)) & 0xFF;
        xh[f]++;
    }
    int nd = n - 1;
    for (int i = 0; i < nd; i++) {
        int64_t d = (int64_t)timings[i+1] - (int64_t)timings[i];
        uint8_t f = 0;
        for (int b = 0; b < 8; b++) f ^= (((uint64_t)d) >> (b*8)) & 0xFF;
        dh[f]++;
    }

    double rs=0,xs=0,ds=0; int rm=0,xm=0,dm=0;
    for (int i = 0; i < 256; i++) {
        if (rh[i]>0) { double p=(double)rh[i]/n; rs -= p*log2(p); }
        if (xh[i]>0) { double p=(double)xh[i]/n; xs -= p*log2(p); }
        if (dh[i]>0) { double p=(double)dh[i]/nd; ds -= p*log2(p); }
        if (rh[i]>rm) rm=rh[i]; if (xh[i]>xm) xm=xh[i]; if (dh[i]>dm) dm=dh[i];
    }

    uint64_t sum=0, tmin=UINT64_MAX, tmax=0;
    for (int i=0; i<n; i++) { sum+=timings[i]; if(timings[i]<tmin) tmin=timings[i]; if(timings[i]>tmax) tmax=timings[i]; }
    double mean = (double)sum/n;
    uint64_t mns = (uint64_t)(mean*tb.numer/tb.denom);

    printf("%s:\n", name);
    printf("  N=%d  Mean=%.0f ticks (≈%llu ns = %.2f ms)  Range=%llu-%llu\n",
           n, mean, mns, (double)mns/1000000.0, tmin, tmax);
    printf("  Raw LSB:        Shannon=%.3f  H∞=%.3f\n", rs, -log2((double)rm/n));
    printf("  XOR-folded:     Shannon=%.3f  H∞=%.3f\n", xs, -log2((double)xm/n));
    printf("  Delta XOR-fold: Shannon=%.3f  H∞=%.3f\n\n", ds, -log2((double)dm/nd));
}

int main(void) {
    printf("# Keychain Write Timing — High-Volume Test\n\n");

    // ===== TEST 1: SecItemAdd + SecItemDelete cycle =====
    {
        uint64_t timings[N_SAMPLES];
        int valid = 0;

        for (int i = 0; i < N_SAMPLES; i++) {
            char label[64];
            snprintf(label, sizeof(label), "oe-entropy-%d", i);
            CFStringRef labelRef = CFStringCreateWithCString(NULL, label, kCFStringEncodingUTF8);

            uint8_t secret[16];
            memset(secret, (uint8_t)i, sizeof(secret));
            CFDataRef secretData = CFDataCreate(NULL, secret, sizeof(secret));

            CFMutableDictionaryRef attrs = CFDictionaryCreateMutable(
                NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
            CFDictionarySetValue(attrs, kSecClass, kSecClassGenericPassword);
            CFDictionarySetValue(attrs, kSecAttrLabel, labelRef);
            CFDictionarySetValue(attrs, kSecValueData, secretData);
            // Use kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly for speed
            CFDictionarySetValue(attrs, kSecAttrAccessible,
                               kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly);

            uint64_t t0 = mach_absolute_time();
            OSStatus status = SecItemAdd(attrs, NULL);
            uint64_t t1 = mach_absolute_time();

            if (status == errSecSuccess) {
                timings[valid++] = t1 - t0;
            }

            // Delete immediately
            CFMutableDictionaryRef query = CFDictionaryCreateMutable(
                NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
            CFDictionarySetValue(query, kSecClass, kSecClassGenericPassword);
            CFDictionarySetValue(query, kSecAttrLabel, labelRef);
            SecItemDelete(query);

            CFRelease(query);
            CFRelease(attrs);
            CFRelease(secretData);
            CFRelease(labelRef);
        }
        if (valid > 100) analyze("SecItemAdd", timings, valid);
    }

    // ===== TEST 2: SecItemDelete timing =====
    {
        // Pre-create items
        for (int i = 0; i < N_SAMPLES; i++) {
            char label[64];
            snprintf(label, sizeof(label), "oe-del-%d", i);
            CFStringRef labelRef = CFStringCreateWithCString(NULL, label, kCFStringEncodingUTF8);
            uint8_t secret[16] = {(uint8_t)i};
            CFDataRef secretData = CFDataCreate(NULL, secret, sizeof(secret));
            CFMutableDictionaryRef attrs = CFDictionaryCreateMutable(
                NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
            CFDictionarySetValue(attrs, kSecClass, kSecClassGenericPassword);
            CFDictionarySetValue(attrs, kSecAttrLabel, labelRef);
            CFDictionarySetValue(attrs, kSecValueData, secretData);
            SecItemAdd(attrs, NULL);
            CFRelease(attrs);
            CFRelease(secretData);
            CFRelease(labelRef);
        }

        uint64_t timings[N_SAMPLES];
        int valid = 0;

        for (int i = 0; i < N_SAMPLES; i++) {
            char label[64];
            snprintf(label, sizeof(label), "oe-del-%d", i);
            CFStringRef labelRef = CFStringCreateWithCString(NULL, label, kCFStringEncodingUTF8);

            CFMutableDictionaryRef query = CFDictionaryCreateMutable(
                NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
            CFDictionarySetValue(query, kSecClass, kSecClassGenericPassword);
            CFDictionarySetValue(query, kSecAttrLabel, labelRef);

            uint64_t t0 = mach_absolute_time();
            OSStatus status = SecItemDelete(query);
            uint64_t t1 = mach_absolute_time();

            if (status == errSecSuccess) {
                timings[valid++] = t1 - t0;
            }

            CFRelease(query);
            CFRelease(labelRef);
        }
        if (valid > 100) analyze("SecItemDelete", timings, valid);
    }

    // ===== TEST 3: SecItemCopyMatching (lookup) timing =====
    {
        // Create one item to look up repeatedly
        const char *label = "oe-lookup-target";
        CFStringRef labelRef = CFStringCreateWithCString(NULL, label, kCFStringEncodingUTF8);
        uint8_t secret[16] = {42};
        CFDataRef secretData = CFDataCreate(NULL, secret, sizeof(secret));
        CFMutableDictionaryRef attrs = CFDictionaryCreateMutable(
            NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        CFDictionarySetValue(attrs, kSecClass, kSecClassGenericPassword);
        CFDictionarySetValue(attrs, kSecAttrLabel, labelRef);
        CFDictionarySetValue(attrs, kSecValueData, secretData);
        SecItemAdd(attrs, NULL);
        CFRelease(attrs);
        CFRelease(secretData);

        uint64_t timings[N_SAMPLES];
        for (int i = 0; i < N_SAMPLES; i++) {
            CFMutableDictionaryRef query = CFDictionaryCreateMutable(
                NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
            CFDictionarySetValue(query, kSecClass, kSecClassGenericPassword);
            CFDictionarySetValue(query, kSecAttrLabel, labelRef);
            CFDictionarySetValue(query, kSecReturnData, kCFBooleanTrue);

            CFTypeRef result = NULL;
            uint64_t t0 = mach_absolute_time();
            SecItemCopyMatching(query, &result);
            uint64_t t1 = mach_absolute_time();
            timings[i] = t1 - t0;

            if (result) CFRelease(result);
            CFRelease(query);
        }
        analyze("SecItemCopyMatching", timings, N_SAMPLES);

        // Cleanup
        CFMutableDictionaryRef cleanup = CFDictionaryCreateMutable(
            NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        CFDictionarySetValue(cleanup, kSecClass, kSecClassGenericPassword);
        CFDictionarySetValue(cleanup, kSecAttrLabel, labelRef);
        SecItemDelete(cleanup);
        CFRelease(cleanup);
        CFRelease(labelRef);
    }

    return 0;
}
