// SMC (System Management Controller) sensor ADC noise
// The LSBs of temperature/voltage/current readings contain ADC quantization noise.
// We probe the SMC via IOKit to read raw sensor values at high frequency.
// The noise floor of the ADC is thermodynamic (Johnson-Nyquist noise).

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <IOKit/IOKitLib.h>
#include <CoreFoundation/CoreFoundation.h>

// SMC data types
typedef struct {
    char key[5];
    uint32_t dataSize;
    uint32_t dataType;
    uint8_t bytes[32];
} SMCVal_t;

typedef struct {
    uint32_t key;
    uint8_t vers[6];
    uint8_t pLimitData[16];
    uint8_t vMajor;
    uint8_t vMinor;
    uint8_t vBuild;
    uint8_t rsvd;
    uint32_t dataSize;
    uint32_t dataType;
    uint8_t dataAttributes;
    uint8_t data[32];
    uint8_t padding[2];
} SMCKeyData_t;

#define KERNEL_INDEX_SMC 2
#define SMC_CMD_READ_KEYINFO 9
#define SMC_CMD_READ_BYTES 5

static io_connect_t conn = 0;

static uint32_t str_to_uint32(const char *str) {
    uint32_t v = 0;
    v = ((uint32_t)str[0] << 24) | ((uint32_t)str[1] << 16) |
        ((uint32_t)str[2] << 8) | (uint32_t)str[3];
    return v;
}

static kern_return_t smc_call(int index, SMCKeyData_t *in, SMCKeyData_t *out) {
    size_t inSize = sizeof(SMCKeyData_t);
    size_t outSize = sizeof(SMCKeyData_t);
    return IOConnectCallStructMethod(conn, index, in, inSize, out, &outSize);
}

static kern_return_t smc_read_key(const char *key, SMCVal_t *val) {
    SMCKeyData_t in = {0}, out = {0};

    in.key = str_to_uint32(key);
    in.data[8] = SMC_CMD_READ_KEYINFO;

    kern_return_t kr = smc_call(KERNEL_INDEX_SMC, &in, &out);
    if (kr != KERN_SUCCESS) return kr;

    val->dataSize = out.dataSize;
    val->dataType = out.dataType;

    memset(&in, 0, sizeof(in));
    in.key = str_to_uint32(key);
    in.data[8] = SMC_CMD_READ_BYTES;
    in.dataSize = val->dataSize;

    kr = smc_call(KERNEL_INDEX_SMC, &in, &out);
    if (kr != KERN_SUCCESS) return kr;

    memcpy(val->bytes, out.data, sizeof(val->bytes));
    strncpy(val->key, key, 4);
    val->key[4] = 0;
    return KERN_SUCCESS;
}

static kern_return_t smc_open(void) {
    io_service_t service = IOServiceGetMatchingService(
        kIOMainPortDefault, IOServiceMatching("AppleSMC"));
    if (!service) return KERN_FAILURE;

    kern_return_t kr = IOServiceOpen(service, mach_task_self(), 0, &conn);
    IOObjectRelease(service);
    return kr;
}

static void smc_close(void) {
    IOServiceClose(conn);
}

// Convert SMC raw bytes to a uint64_t for analysis
static uint64_t smc_val_to_uint64(SMCVal_t *val) {
    uint64_t v = 0;
    for (uint32_t i = 0; i < val->dataSize && i < 8; i++) {
        v = (v << 8) | val->bytes[i];
    }
    return v;
}

#define N_SAMPLES 20000
#define N_KEYS 8

int main(void) {
    // SMC temperature/voltage/current keys to probe
    // TC0P = CPU proximity temp, TC0D = CPU die temp,
    // VCAC = CPU core voltage, IC0C = CPU current, etc.
    const char *keys[] = {
        "TC0P", "TC0D", "TC0E", "TC0F",  // temperatures
        "VCAC", "VC0C", "IC0C", "IC1C",  // voltages/currents
    };

    if (smc_open() != KERN_SUCCESS) {
        fprintf(stderr, "Failed to open SMC connection. Try running with sudo.\n");
        return 1;
    }

    printf("# SMC Sensor ADC Noise Probe\n");
    printf("# Collecting %d samples from %d SMC keys...\n\n", N_SAMPLES, N_KEYS);

    // For each key, collect rapid samples and analyze LSB entropy
    for (int k = 0; k < N_KEYS; k++) {
        uint64_t samples[N_SAMPLES];
        int valid = 0;

        for (int i = 0; i < N_SAMPLES; i++) {
            SMCVal_t val = {0};
            if (smc_read_key(keys[k], &val) == KERN_SUCCESS && val.dataSize > 0) {
                samples[valid++] = smc_val_to_uint64(&val);
            }
        }

        if (valid < 100) {
            printf("Key %s: only %d valid reads, skipping\n", keys[k], valid);
            continue;
        }

        // Compute deltas
        int64_t deltas[N_SAMPLES];
        int n_deltas = 0;
        for (int i = 1; i < valid; i++) {
            deltas[n_deltas++] = (int64_t)samples[i] - (int64_t)samples[i-1];
        }

        // Count unique delta values (histogram)
        int histogram[256] = {0};
        for (int i = 0; i < n_deltas; i++) {
            uint8_t lsb = (uint8_t)(deltas[i] & 0xFF);
            histogram[lsb]++;
        }

        // Shannon entropy of LSBs
        double shannon = 0.0;
        for (int i = 0; i < 256; i++) {
            if (histogram[i] > 0) {
                double p = (double)histogram[i] / n_deltas;
                shannon -= p * log2(p);
            }
        }

        // Min-entropy = -log2(max_probability)
        int max_count = 0;
        for (int i = 0; i < 256; i++) {
            if (histogram[i] > max_count) max_count = histogram[i];
        }
        double min_entropy = -log2((double)max_count / n_deltas);

        // Also analyze raw value LSBs (not deltas)
        int raw_hist[256] = {0};
        for (int i = 0; i < valid; i++) {
            raw_hist[samples[i] & 0xFF]++;
        }
        double raw_shannon = 0.0;
        int raw_max = 0;
        for (int i = 0; i < 256; i++) {
            if (raw_hist[i] > 0) {
                double p = (double)raw_hist[i] / valid;
                raw_shannon -= p * log2(p);
            }
            if (raw_hist[i] > raw_max) raw_max = raw_hist[i];
        }
        double raw_min_entropy = -log2((double)raw_max / valid);

        printf("Key: %s | Samples: %d\n", keys[k], valid);
        printf("  Delta LSB: Shannon=%.3f  H∞=%.3f  unique_vals=%d\n",
               shannon, min_entropy,
               ({int c=0; for(int i=0;i<256;i++) if(histogram[i]>0) c++; c;}));
        printf("  Raw LSB:   Shannon=%.3f  H∞=%.3f  unique_vals=%d\n",
               raw_shannon, raw_min_entropy,
               ({int c=0; for(int i=0;i<256;i++) if(raw_hist[i]>0) c++; c;}));
        printf("  Sample range: %llu - %llu\n",
               samples[0], samples[valid-1]);
        printf("\n");
    }

    smc_close();
    return 0;
}
