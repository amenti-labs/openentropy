// Cross-Correlation Test
// Simultaneously collect from new and existing sources,
// compute Pearson correlation to check independence.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <mach/mach.h>
#include <sys/mman.h>
#include <pthread.h>
#include <Security/Security.h>
#include <CoreFoundation/CoreFoundation.h>

#define N 5000
#define ARRAY_SIZE (16 * 1024 * 1024)

static double pearson(uint64_t *a, uint64_t *b, int n) {
    double ma = 0, mb = 0;
    for (int i = 0; i < n; i++) { ma += a[i]; mb += b[i]; }
    ma /= n; mb /= n;

    double num = 0, da = 0, db = 0;
    for (int i = 0; i < n; i++) {
        double x = a[i] - ma, y = b[i] - mb;
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if (da < 1e-15 || db < 1e-15) return 0;
    return num / sqrt(da * db);
}

static inline uint64_t read_counter(void) {
    uint64_t val;
    __asm__ volatile("isb\nmrs %0, CNTVCT_EL0" : "=r"(val));
    return val;
}

static inline void dmb(void) {
    __asm__ volatile("dmb sy" ::: "memory");
}

int main(void) {
    printf("# Cross-Correlation Matrix — New vs Existing Sources\n\n");

    // === Collect DMP confusion timings ===
    uint64_t *dmp_t = malloc(N * sizeof(uint64_t));
    {
        uint64_t *array = mmap(NULL, ARRAY_SIZE, PROT_READ | PROT_WRITE,
                               MAP_PRIVATE | MAP_ANON, -1, 0);
        uint64_t base = (uint64_t)array;
        size_t n_el = ARRAY_SIZE / sizeof(uint64_t);
        uint64_t lcg = mach_absolute_time() | 1;
        for (size_t i = 0; i < n_el; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            array[i] = base + ((lcg >> 16) % n_el) * 8;
        }
        volatile uint64_t sink = 0;
        for (int i = 0; i < N; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t idx = (lcg >> 16) % (n_el - 256);
            dmb();
            uint64_t t0 = read_counter();
            uint64_t val = array[idx];
            size_t next = (val - base) / 8;
            if (next < n_el) {
                uint64_t v2 = array[next];
                size_t n2 = (v2 - base) / 8;
                if (n2 < n_el) { sink += array[n2]; sink += array[idx > 64 ? idx-64 : 0]; }
            }
            dmb();
            uint64_t t1 = read_counter();
            dmp_t[i] = t1 - t0;
        }
        munmap(array, ARRAY_SIZE);
    }

    // === Collect keychain timings ===
    uint64_t *kc_t = malloc(N * sizeof(uint64_t));
    {
        CFStringRef label = CFStringCreateWithCString(NULL, "oe-xcorr-probe", kCFStringEncodingUTF8);
        uint8_t secret[16] = {0x42};
        CFDataRef sd = CFDataCreate(NULL, secret, 16);
        CFMutableDictionaryRef a = CFDictionaryCreateMutable(
            NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        CFDictionarySetValue(a, kSecClass, kSecClassGenericPassword);
        CFDictionarySetValue(a, kSecAttrLabel, label);
        CFDictionarySetValue(a, kSecValueData, sd);

        CFMutableDictionaryRef d = CFDictionaryCreateMutable(
            NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        CFDictionarySetValue(d, kSecClass, kSecClassGenericPassword);
        CFDictionarySetValue(d, kSecAttrLabel, label);
        SecItemDelete((CFDictionaryRef)d);
        SecItemAdd((CFDictionaryRef)a, NULL);
        CFRelease(a); CFRelease(sd);

        CFMutableDictionaryRef q = CFDictionaryCreateMutable(
            NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        CFDictionarySetValue(q, kSecClass, kSecClassGenericPassword);
        CFDictionarySetValue(q, kSecAttrLabel, label);
        CFDictionarySetValue(q, kSecReturnData, kCFBooleanTrue);

        for (int i = 0; i < N; i++) {
            CFTypeRef result = NULL;
            uint64_t t0 = mach_absolute_time();
            SecItemCopyMatching((CFDictionaryRef)q, &result);
            uint64_t t1 = mach_absolute_time();
            kc_t[i] = t1 - t0;
            if (result) CFRelease(result);
        }
        CFRelease(q);
        SecItemDelete((CFDictionaryRef)d);
        CFRelease(d); CFRelease(label);
    }

    // === Collect cache contention (existing source) ===
    uint64_t *cache_t = malloc(N * sizeof(uint64_t));
    {
        size_t stride = 64;
        size_t arr_size = 4 * 1024 * 1024;
        volatile uint8_t *arr = mmap(NULL, arr_size, PROT_READ | PROT_WRITE,
                                     MAP_PRIVATE | MAP_ANON, -1, 0);
        uint64_t lcg = mach_absolute_time() | 1;
        for (int i = 0; i < N; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t off1 = (lcg >> 16) % arr_size;
            lcg = lcg * 6364136223846793005ULL + 1;
            size_t off2 = (lcg >> 16) % arr_size;

            dmb();
            uint64_t t0 = read_counter();
            arr[off1]++;
            arr[off2]++;
            dmb();
            uint64_t t1 = read_counter();
            cache_t[i] = t1 - t0;
        }
        munmap((void*)arr, arr_size);
    }

    // === Collect mach_ipc (existing source) ===
    uint64_t *ipc_t = malloc(N * sizeof(uint64_t));
    {
        for (int i = 0; i < N; i++) {
            mach_port_t port;
            uint64_t t0 = mach_absolute_time();
            mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &port);
            mach_port_deallocate(mach_task_self(), port);
            uint64_t t1 = mach_absolute_time();
            ipc_t[i] = t1 - t0;
        }
    }

    // === Collect tlb_shootdown (existing source) ===
    uint64_t *tlb_t = malloc(N * sizeof(uint64_t));
    {
        size_t page_size = 4096;
        size_t region = 256 * page_size;
        void *addr = mmap(NULL, region, PROT_READ | PROT_WRITE,
                          MAP_PRIVATE | MAP_ANON, -1, 0);
        for (size_t p = 0; p < 256; p++) {
            ((volatile uint8_t*)addr)[p * page_size] = 0xAA;
        }
        uint64_t lcg = mach_absolute_time() | 1;
        for (int i = 0; i < N; i++) {
            lcg = lcg * 6364136223846793005ULL + 1;
            int pages = 8 + ((lcg >> 32) % 121);
            uint64_t t0 = mach_absolute_time();
            mprotect(addr, pages * page_size, PROT_READ);
            mprotect(addr, pages * page_size, PROT_READ | PROT_WRITE);
            uint64_t t1 = mach_absolute_time();
            tlb_t[i] = t1 - t0;
        }
        munmap(addr, region);
    }

    // === Compute correlation matrix ===
    const char *names[] = {"dmp_confusion", "keychain", "cache_contention", "mach_ipc", "tlb_shootdown"};
    uint64_t *streams[] = {dmp_t, kc_t, cache_t, ipc_t, tlb_t};
    int n_streams = 5;

    printf("Correlation Matrix (Pearson r, %d samples each):\n\n", N);
    printf("%-18s", "");
    for (int i = 0; i < n_streams; i++) printf("  %-15s", names[i]);
    printf("\n");

    for (int i = 0; i < n_streams; i++) {
        printf("%-18s", names[i]);
        for (int j = 0; j < n_streams; j++) {
            double r = pearson(streams[i], streams[j], N);
            printf("  %+.4f        ", r);
        }
        printf("\n");
    }

    printf("\nInterpretation:\n");
    printf("  |r| < 0.05: No correlation (independent)\n");
    printf("  |r| 0.05-0.10: Negligible\n");
    printf("  |r| 0.10-0.30: Weak correlation\n");
    printf("  |r| > 0.30: SIGNIFICANT — sources may share entropy domain\n\n");

    // Flag concerning correlations
    printf("Flagged pairs (|r| > 0.10):\n");
    int any_flagged = 0;
    for (int i = 0; i < n_streams; i++) {
        for (int j = i + 1; j < n_streams; j++) {
            double r = pearson(streams[i], streams[j], N);
            if (fabs(r) > 0.10) {
                printf("  *** %s × %s: r=%.4f ***\n", names[i], names[j], r);
                any_flagged = 1;
            }
        }
    }
    if (!any_flagged) printf("  (none — all sources appear independent)\n");

    for (int i = 0; i < n_streams; i++) free(streams[i]);
    return 0;
}
