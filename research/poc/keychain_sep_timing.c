// Keychain/Secure Enclave Timing — More targeted approach
// Instead of SecRandomCopyBytes (which may bypass SEP), directly use
// Security.framework Keychain operations that MUST go through SEP.
// Also test: SecKeyCreateRandomKey timing, SecAccessControlCreate, etc.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <Security/Security.h>
#include <CoreFoundation/CoreFoundation.h>
#include <CommonCrypto/CommonDigest.h>

#define N_SAMPLES 10000

static void analyze_timings(const char *name, uint64_t *timings, int n) {
    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // Raw LSB
    int hist[256] = {0};
    for (int i = 0; i < n; i++) hist[timings[i] & 0xFF]++;
    double sh = 0; int mx = 0;
    for (int i = 0; i < 256; i++) {
        if (hist[i] > 0) { double p = (double)hist[i]/n; sh -= p*log2(p); }
        if (hist[i] > mx) mx = hist[i];
    }

    // XOR-fold
    int xh[256] = {0};
    for (int i = 0; i < n; i++) {
        uint8_t f = 0;
        for (int b = 0; b < 8; b++) f ^= (timings[i] >> (b*8)) & 0xFF;
        xh[f]++;
    }
    double xs = 0; int xm = 0;
    for (int i = 0; i < 256; i++) {
        if (xh[i] > 0) { double p = (double)xh[i]/n; xs -= p*log2(p); }
        if (xh[i] > xm) xm = xh[i];
    }

    // Delta XOR-fold
    int dh[256] = {0};
    int nd = n - 1;
    for (int i = 0; i < nd; i++) {
        int64_t d = (int64_t)timings[i+1] - (int64_t)timings[i];
        uint8_t f = 0;
        for (int b = 0; b < 8; b++) f ^= (((uint64_t)d) >> (b*8)) & 0xFF;
        dh[f]++;
    }
    double ds = 0; int dm = 0;
    for (int i = 0; i < 256; i++) {
        if (dh[i] > 0) { double p = (double)dh[i]/nd; ds -= p*log2(p); }
        if (dh[i] > dm) dm = dh[i];
    }

    uint64_t sum = 0, tmin = UINT64_MAX, tmax = 0;
    for (int i = 0; i < n; i++) {
        sum += timings[i];
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
    }
    double mean = (double)sum / n;
    uint64_t mns = (uint64_t)(mean * tb.numer / tb.denom);

    printf("%s:\n", name);
    printf("  Samples: %d  Mean: %.0f ticks (≈%llu ns)  Range: %llu-%llu\n",
           n, mean, mns, tmin, tmax);
    printf("  Raw LSB:        Shannon=%.3f  H∞=%.3f\n", sh, -log2((double)mx/n));
    printf("  XOR-folded:     Shannon=%.3f  H∞=%.3f\n", xs, -log2((double)xm/n));
    printf("  Delta XOR-fold: Shannon=%.3f  H∞=%.3f\n\n", ds, -log2((double)dm/nd));
}

int main(void) {
    printf("# Keychain / SEP Timing Deep Probe\n\n");

    // ===== TEST 1: SecRandomCopyBytes timing (SEP random path) =====
    {
        uint64_t timings[N_SAMPLES];
        uint8_t buf[32];
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t t0 = mach_absolute_time();
            SecRandomCopyBytes(kSecRandomDefault, 32, buf);
            uint64_t t1 = mach_absolute_time();
            timings[i] = t1 - t0;
        }
        analyze_timings("SecRandomCopyBytes(32)", timings, N_SAMPLES);
    }

    // ===== TEST 2: SecRandomCopyBytes with 1 byte (minimal payload) =====
    {
        uint64_t timings[N_SAMPLES];
        uint8_t buf;
        for (int i = 0; i < N_SAMPLES; i++) {
            uint64_t t0 = mach_absolute_time();
            SecRandomCopyBytes(kSecRandomDefault, 1, &buf);
            uint64_t t1 = mach_absolute_time();
            timings[i] = t1 - t0;
        }
        analyze_timings("SecRandomCopyBytes(1)", timings, N_SAMPLES);
    }

    // ===== TEST 3: SecItemAdd/Delete cycle timing (full Keychain path) =====
    {
        uint64_t timings[N_SAMPLES / 10];  // Slower, fewer samples
        int valid = 0;

        for (int i = 0; i < N_SAMPLES / 10; i++) {
            char label[64];
            snprintf(label, sizeof(label), "entropy-test-%d", i);
            CFStringRef labelRef = CFStringCreateWithCString(NULL, label, kCFStringEncodingUTF8);

            // Create a small keychain item
            uint8_t secret[16] = {(uint8_t)i};
            CFDataRef secretData = CFDataCreate(NULL, secret, sizeof(secret));

            CFMutableDictionaryRef attrs = CFDictionaryCreateMutable(
                NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
            CFDictionarySetValue(attrs, kSecClass, kSecClassGenericPassword);
            CFDictionarySetValue(attrs, kSecAttrLabel, labelRef);
            CFDictionarySetValue(attrs, kSecValueData, secretData);

            uint64_t t0 = mach_absolute_time();
            OSStatus status = SecItemAdd(attrs, NULL);
            uint64_t t1 = mach_absolute_time();

            if (status == errSecSuccess || status == errSecDuplicateItem) {
                timings[valid++] = t1 - t0;
            }

            // Clean up — delete the item
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
        if (valid > 10) {
            analyze_timings("SecItemAdd (Keychain write)", timings, valid);
        }
    }

    // ===== TEST 4: SecAccessControlCreate timing =====
    {
        uint64_t timings[N_SAMPLES];
        for (int i = 0; i < N_SAMPLES; i++) {
            CFErrorRef error = NULL;
            uint64_t t0 = mach_absolute_time();
            SecAccessControlRef ac = SecAccessControlCreateWithFlags(
                NULL,
                kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
                kSecAccessControlPrivateKeyUsage,
                &error);
            uint64_t t1 = mach_absolute_time();
            timings[i] = t1 - t0;
            if (ac) CFRelease(ac);
            if (error) CFRelease(error);
        }
        analyze_timings("SecAccessControlCreate", timings, N_SAMPLES);
    }

    // ===== TEST 5: SecCertificateCopyPublicKey on a self-signed cert =====
    // Creating and parsing certs goes through the crypto engine
    {
        uint64_t timings[N_SAMPLES];
        for (int i = 0; i < N_SAMPLES; i++) {
            // Just time the overhead of a crypto framework call
            uint8_t hash_input[32];
            memset(hash_input, (uint8_t)i, sizeof(hash_input));

            uint64_t t0 = mach_absolute_time();
            // CC_SHA256 is hardware-accelerated on Apple Silicon
            // The timing reflects crypto engine state
            uint8_t hash_output[32];
            CC_SHA256(hash_input, sizeof(hash_input), hash_output);
            uint64_t t1 = mach_absolute_time();
            timings[i] = t1 - t0;
        }
        analyze_timings("CC_SHA256 (crypto engine)", timings, N_SAMPLES);
    }

    return 0;
}
