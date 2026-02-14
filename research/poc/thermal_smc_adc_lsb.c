// SMC Sensor ADC Raw LSBs — Thermal noise in analog-to-digital conversion
//
// The System Management Controller (SMC) on Apple Silicon continuously
// samples temperature, voltage, and current sensors via integrated ADCs.
// The LSBs of these readings contain ADC quantization noise that is
// fundamentally thermal (Johnson-Nyquist noise in the sensor + ADC input).
//
// This PoC reads many SMC keys at high frequency and analyzes the
// per-key and cross-key LSB entropy. We probe more keys than the existing
// smc_sensor_noise.c and also look at timing jitter between reads.
//
// Build: cc -O2 -o thermal_smc_adc_lsb thermal_smc_adc_lsb.c \
//        -framework IOKit -framework CoreFoundation -lm

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <IOKit/IOKitLib.h>
#include <CoreFoundation/CoreFoundation.h>

// SMC structures (matching Apple's private interface)
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
    return ((uint32_t)str[0] << 24) | ((uint32_t)str[1] << 16) |
           ((uint32_t)str[2] << 8) | (uint32_t)str[3];
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

static uint64_t smc_val_to_uint64(SMCVal_t *val) {
    uint64_t v = 0;
    for (uint32_t i = 0; i < val->dataSize && i < 8; i++)
        v = (v << 8) | val->bytes[i];
    return v;
}

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
    printf("  %s: Shannon=%.3f  H∞=%.3f  unique=%d/256\n",
           label, shannon, min_entropy, unique);
}

#define N_SAMPLES 10000
#define N_KEYS 16

int main(void) {
    printf("# SMC Sensor ADC Raw LSBs — Thermal Noise Probe\n\n");

    // Expanded key set: temperatures, voltages, currents, fans, power
    const char *keys[N_KEYS] = {
        "TC0P", "TC0D", "TC0E", "TC0F",  // CPU temps
        "TCGC", "TCSA", "TCXC", "TW0P",  // GPU, SA, aux temps
        "VCAC", "VC0C", "VD0R", "VP0R",  // voltages
        "IC0C", "IC1C", "IC0R", "IPBR",  // currents
    };

    if (smc_open() != KERN_SUCCESS) {
        fprintf(stderr, "Failed to open SMC connection. Try running with sudo.\n");
        return 1;
    }

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // Collect all-keys interleaved data for cross-key analysis
    uint8_t *cross_key_lsbs = malloc(N_SAMPLES * N_KEYS);
    int cross_count = 0;
    uint64_t *read_timings = malloc(N_SAMPLES * N_KEYS * sizeof(uint64_t));
    int timing_count = 0;

    for (int k = 0; k < N_KEYS; k++) {
        // Quick check if this key exists
        SMCVal_t test = {0};
        if (smc_read_key(keys[k], &test) != KERN_SUCCESS || test.dataSize == 0) {
            printf("Key %s: not available, skipping\n", keys[k]);
            continue;
        }

        uint64_t raw_values[N_SAMPLES];
        uint64_t timings[N_SAMPLES];
        int valid = 0;

        for (int i = 0; i < N_SAMPLES; i++) {
            SMCVal_t val = {0};
            uint64_t t0 = mach_absolute_time();
            kern_return_t kr = smc_read_key(keys[k], &val);
            uint64_t t1 = mach_absolute_time();

            if (kr == KERN_SUCCESS && val.dataSize > 0) {
                raw_values[valid] = smc_val_to_uint64(&val);
                timings[valid] = t1 - t0;
                read_timings[timing_count++] = t1 - t0;

                // Store LSB for cross-key analysis
                cross_key_lsbs[cross_count++] = val.bytes[val.dataSize > 1 ? val.dataSize - 1 : 0];
                valid++;
            }
        }

        if (valid < 100) {
            printf("Key %s: only %d valid reads, skipping\n", keys[k], valid);
            continue;
        }

        printf("\n--- Key: %s | %d valid reads, dataSize=%u ---\n",
               keys[k], valid, test.dataSize);
        printf("  Value range: %llu - %llu\n", raw_values[0], raw_values[valid-1]);

        // Raw value LSBs
        uint8_t *lsbs = malloc(valid);
        for (int i = 0; i < valid; i++) lsbs[i] = raw_values[i] & 0xFF;
        analyze_entropy("Raw LSBs", lsbs, valid);

        // Delta LSBs
        uint8_t *deltas = malloc(valid - 1);
        for (int i = 0; i < valid - 1; i++) {
            int64_t d = (int64_t)raw_values[i+1] - (int64_t)raw_values[i];
            deltas[i] = (uint8_t)(d & 0xFF);
        }
        analyze_entropy("Delta LSBs", deltas, valid - 1);

        // Read timing LSBs (SMC bus timing jitter)
        uint8_t *timing_lsbs = malloc(valid);
        for (int i = 0; i < valid; i++) timing_lsbs[i] = timings[i] & 0xFF;
        analyze_entropy("Read timing LSBs", timing_lsbs, valid);

        // Timing XOR-fold
        uint8_t *timing_xor = malloc(valid);
        for (int i = 0; i < valid; i++) {
            uint64_t t = timings[i];
            timing_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                            ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
        }
        analyze_entropy("Read timing XOR-fold", timing_xor, valid);

        free(lsbs);
        free(deltas);
        free(timing_lsbs);
        free(timing_xor);
    }

    // Cross-key interleaved LSB analysis
    if (cross_count > 100) {
        printf("\n=== Cross-key interleaved LSB analysis (%d bytes) ===\n", cross_count);
        analyze_entropy("Interleaved LSBs", cross_key_lsbs, cross_count);
    }

    // All read timings combined
    if (timing_count > 100) {
        uint8_t *all_timing_xor = malloc(timing_count);
        for (int i = 0; i < timing_count; i++) {
            uint64_t t = read_timings[i];
            all_timing_xor[i] = (t & 0xFF) ^ ((t >> 8) & 0xFF) ^
                                ((t >> 16) & 0xFF) ^ ((t >> 24) & 0xFF);
        }
        printf("\n=== All read timings combined (%d samples) ===\n", timing_count);
        analyze_entropy("All timing XOR-fold", all_timing_xor, timing_count);
        free(all_timing_xor);
    }

    free(cross_key_lsbs);
    free(read_timings);
    smc_close();
    return 0;
}
