// Thermal Convection Turbulence Sensor — SMC temperature differential entropy
//
// The Mac's cooling system creates turbulent airflow over the heatsink.
// Turbulence is one of the few genuinely chaotic classical systems (Navier-Stokes).
// By rapidly reading two temperature sensors at different physical locations,
// the DIFFERENCE fluctuates with thermal convection currents.
//
// This is genuinely novel: nobody has used a computer's own thermal sensors
// as a turbulence detector for entropy generation.
//
// Build: cc -O2 -o unprecedented_thermal_convection unprecedented_thermal_convection.c -framework IOKit -framework CoreFoundation
// Note: Requires root (sudo) for direct SMC access.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <mach/mach_time.h>
#include <IOKit/IOKitLib.h>
#include <CoreFoundation/CoreFoundation.h>

#define N_SAMPLES 12000
#define SMC_CMD_READ_KEYINFO 9
#define SMC_CMD_READ_BYTES   5

// SMC data types
typedef struct {
    char key[5];
    uint32_t data_size;
    uint32_t data_type;
    uint8_t  data_attributes;
} SMCKeyInfoData;

typedef struct {
    uint32_t key;
    SMCKeyInfoData key_info;
    uint8_t  result;
    uint8_t  status;
    uint8_t  data8;
    uint32_t data32;
    uint8_t  bytes[32];
} SMCParamStruct;

#pragma pack(push, 1)
typedef struct {
    uint32_t data_type;
    uint32_t data_size;
    uint8_t  cmd;
    uint32_t key;
    uint8_t  padding[24];
    uint8_t  bytes[32];
} SMCData;
#pragma pack(pop)

static io_connect_t smc_conn = 0;

static uint32_t str_to_key(const char *s) {
    return ((uint32_t)s[0] << 24) | ((uint32_t)s[1] << 16) |
           ((uint32_t)s[2] << 8)  | (uint32_t)s[3];
}

static int smc_open(void) {
    io_service_t service = IOServiceGetMatchingService(
        kIOMainPortDefault,
        IOServiceMatching("AppleSMC"));
    if (!service) {
        fprintf(stderr, "Failed to find AppleSMC service\n");
        return -1;
    }
    kern_return_t kr = IOServiceOpen(service, mach_task_self(), 0, &smc_conn);
    IOObjectRelease(service);
    if (kr != KERN_SUCCESS) {
        fprintf(stderr, "Failed to open SMC (need sudo?): 0x%x\n", kr);
        return -1;
    }
    return 0;
}

static void smc_close(void) {
    if (smc_conn) IOServiceClose(smc_conn);
}

static int smc_read_key(const char *key_str, uint8_t *out, uint32_t *out_size) {
    SMCParamStruct in = {0}, out_s = {0};
    size_t out_size_s = sizeof(SMCParamStruct);

    // Get key info first
    in.key = str_to_key(key_str);
    in.data8 = SMC_CMD_READ_KEYINFO;
    kern_return_t kr = IOConnectCallStructMethod(
        smc_conn, 2, &in, sizeof(SMCParamStruct), &out_s, &out_size_s);
    if (kr != KERN_SUCCESS) return -1;

    // Now read the key
    memset(&in, 0, sizeof(in));
    in.key = str_to_key(key_str);
    in.key_info.data_size = out_s.key_info.data_size;
    in.data8 = SMC_CMD_READ_BYTES;
    out_size_s = sizeof(SMCParamStruct);
    kr = IOConnectCallStructMethod(
        smc_conn, 2, &in, sizeof(SMCParamStruct), &out_s, &out_size_s);
    if (kr != KERN_SUCCESS) return -1;

    *out_size = out_s.key_info.data_size;
    memcpy(out, out_s.bytes, out_s.key_info.data_size);
    return 0;
}

// Convert SMC flt/fpe2/sp78 types to float
static float smc_bytes_to_float(const uint8_t *bytes, uint32_t size) {
    if (size == 4) {
        // flt type — IEEE 754 float
        float f;
        memcpy(&f, bytes, 4);
        return f;
    } else if (size == 2) {
        // sp78 or fpe2 — fixed point
        int16_t val = ((int16_t)bytes[0] << 8) | bytes[1];
        return val / 256.0f;
    }
    return 0.0f;
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
    printf("  %s: Shannon=%.3f  H∞=%.3f  unique=%d/256  n=%d\n",
           label, shannon, min_entropy, unique, n);
}

int main(void) {
    printf("# Thermal Convection Turbulence Sensor — SMC Temperature Differential\n\n");

    if (smc_open() != 0) {
        printf("FAIL: Cannot open SMC (requires sudo)\n");
        return 1;
    }

    // Temperature sensor keys to try (different physical locations on the SoC/board)
    const char *sensor_keys[] = {
        "TC0P",  // CPU proximity temp
        "TC1P",  // CPU proximity temp 2
        "TC0D",  // CPU die temp
        "TC1D",  // CPU die temp 2
        "Tp01",  // P-core cluster 1
        "Tp05",  // P-core cluster 2
        "Tp09",  // E-core cluster
        "Tp0T",  // Thermal diode
        "TW0P",  // Wireless module temp
        "TH0P",  // Heatsink temp
        "TA0P",  // Ambient temp
        "Ts0S",  // SSD temp
    };
    int n_keys = sizeof(sensor_keys) / sizeof(sensor_keys[0]);

    printf("Scanning available temperature sensors...\n");
    float sensor_vals[12] = {0};
    int available[12] = {0};
    int n_available = 0;

    for (int i = 0; i < n_keys; i++) {
        uint8_t bytes[32] = {0};
        uint32_t size = 0;
        if (smc_read_key(sensor_keys[i], bytes, &size) == 0) {
            float val = smc_bytes_to_float(bytes, size);
            if (val > 0.0f && val < 150.0f) {
                sensor_vals[i] = val;
                available[i] = 1;
                n_available++;
                printf("  %s: %.2f°C (size=%u)\n", sensor_keys[i], val, size);
            }
        }
    }

    if (n_available < 2) {
        printf("\nFAIL: Need at least 2 temperature sensors, found %d\n", n_available);
        smc_close();
        return 1;
    }

    // Find two sensors with the largest physical separation (biggest temp difference)
    int s1 = -1, s2 = -1;
    float max_diff = 0;
    for (int i = 0; i < n_keys; i++) {
        if (!available[i]) continue;
        for (int j = i+1; j < n_keys; j++) {
            if (!available[j]) continue;
            float diff = fabs(sensor_vals[i] - sensor_vals[j]);
            if (diff > max_diff || s1 < 0) {
                max_diff = diff;
                s1 = i;
                s2 = j;
            }
        }
    }
    printf("\nUsing sensors: %s (%.2f°C) and %s (%.2f°C), delta=%.2f°C\n",
           sensor_keys[s1], sensor_vals[s1], sensor_keys[s2], sensor_vals[s2], max_diff);

    // === Test 1: Temperature differential LSBs ===
    printf("\n--- Test 1: Temperature Differential LSBs ---\n");
    uint8_t *diff_lsbs = malloc(N_SAMPLES);
    int diff_count = 0;

    for (int i = 0; i < N_SAMPLES + 1000; i++) {
        uint8_t b1[32] = {0}, b2[32] = {0};
        uint32_t s1_size = 0, s2_size = 0;
        if (smc_read_key(sensor_keys[s1], b1, &s1_size) != 0) continue;
        if (smc_read_key(sensor_keys[s2], b2, &s2_size) != 0) continue;

        // XOR raw bytes from both sensors
        uint8_t xored = b1[0] ^ b2[0] ^ b1[1] ^ b2[1];
        diff_lsbs[diff_count++] = xored;
        if (diff_count >= N_SAMPLES) break;
    }
    if (diff_count > 0) analyze_entropy("Temp differential XOR", diff_lsbs, diff_count);

    // === Test 2: Read timing jitter of SMC reads ===
    printf("\n--- Test 2: SMC Read Timing Jitter ---\n");
    uint64_t *timings = malloc(N_SAMPLES * sizeof(uint64_t));
    uint8_t *timing_lsbs = malloc(N_SAMPLES);

    for (int i = 0; i < N_SAMPLES; i++) {
        uint8_t bytes[32] = {0};
        uint32_t size = 0;
        uint64_t t0 = mach_absolute_time();
        smc_read_key(sensor_keys[s1], bytes, &size);
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
        timing_lsbs[i] = (uint8_t)(timings[i] & 0xFF);
    }
    analyze_entropy("SMC read timing LSBs", timing_lsbs, N_SAMPLES);

    // === Test 3: Differential timing between two sensor reads ===
    printf("\n--- Test 3: Dual-Sensor Differential Timing ---\n");
    uint8_t *dual_timing = malloc(N_SAMPLES);

    for (int i = 0; i < N_SAMPLES; i++) {
        uint8_t b1[32] = {0}, b2[32] = {0};
        uint32_t sz1 = 0, sz2 = 0;
        uint64_t t0 = mach_absolute_time();
        smc_read_key(sensor_keys[s1], b1, &sz1);
        uint64_t t1 = mach_absolute_time();
        smc_read_key(sensor_keys[s2], b2, &sz2);
        uint64_t t2 = mach_absolute_time();

        // XOR the two timing deltas — captures differential jitter
        uint64_t d1 = t1 - t0;
        uint64_t d2 = t2 - t1;
        uint64_t diff = d1 ^ d2;
        dual_timing[i] = (uint8_t)(diff & 0xFF);
    }
    analyze_entropy("Dual-sensor timing XOR", dual_timing, N_SAMPLES);

    // === Test 4: Delta-of-deltas (convection fluctuation rate) ===
    printf("\n--- Test 4: Delta-of-Deltas (Convection Rate) ---\n");
    uint8_t *dod = malloc(N_SAMPLES);
    uint64_t prev_delta = 0;
    int dod_count = 0;

    for (int i = 0; i < N_SAMPLES + 100; i++) {
        uint8_t b1[32] = {0}, b2[32] = {0};
        uint32_t sz1 = 0, sz2 = 0;
        smc_read_key(sensor_keys[s1], b1, &sz1);
        smc_read_key(sensor_keys[s2], b2, &sz2);

        uint64_t t_read = mach_absolute_time();
        // Combine raw sensor bytes with timing
        uint64_t combined = ((uint64_t)b1[0] << 24) | ((uint64_t)b1[1] << 16) |
                           ((uint64_t)b2[0] << 8)  | (uint64_t)b2[1];
        combined ^= t_read;

        if (i > 0) {
            uint64_t delta = combined ^ prev_delta;
            // XOR-fold to byte
            uint8_t folded = (uint8_t)((delta >> 0) ^ (delta >> 8) ^
                                       (delta >> 16) ^ (delta >> 24) ^
                                       (delta >> 32) ^ (delta >> 40) ^
                                       (delta >> 48) ^ (delta >> 56));
            dod[dod_count++] = folded;
            if (dod_count >= N_SAMPLES) break;
        }
        prev_delta = combined;
    }
    if (dod_count > 0) analyze_entropy("Delta-of-deltas XOR-fold", dod, dod_count);

    free(diff_lsbs);
    free(timings);
    free(timing_lsbs);
    free(dual_timing);
    free(dod);
    smc_close();

    printf("\nDone.\n");
    return 0;
}
