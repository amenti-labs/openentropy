// validate_sensor_noise.c — IORegistry sensor noise entropy validation
// Mechanism: Run `ioreg -l -w0` twice with 50ms delay. Parse all "key" = number patterns.
//            Compute deltas for keys that changed. XOR consecutive deltas, extract bytes.
//            Slow source (~100ms per ioreg), capped at 500/200 samples.
// Cross-correlate with: ioregistry (same ioreg data source)
// Compile: cc -O2 -o validate_sensor_noise validate_sensor_noise.c -lm

#include "validate_common.h"

#define SENSOR_LARGE_N  500
#define SENSOR_TRIAL_N  200
#define MAX_KEYS        8192
#define MAX_KEY_LEN     128

typedef struct {
    char name[MAX_KEY_LEN];
    int64_t value;
} KeyVal;

// Parse ioreg output for "key" = <number> patterns, return count
static int parse_ioreg(KeyVal *kvs, int max_kvs) {
    FILE *fp = popen("ioreg -l -w0 2>/dev/null", "r");
    if (!fp) return 0;

    char line[4096];
    int count = 0;

    while (fgets(line, sizeof(line), fp) && count < max_kvs) {
        // Look for patterns like "KeyName" = 12345
        char *eq = strstr(line, "\" = ");
        if (!eq) continue;

        // Find the opening quote for the key name
        char *q2 = eq;  // points to the closing quote
        char *q1 = NULL;
        for (char *p = line; p < q2; p++) {
            if (*p == '"') q1 = p;
        }
        // q1 is the last quote before eq, but we need the one before the closing quote
        // Actually find the pair: look backwards from eq for "
        char *closing_quote = eq; // eq points to `" = `
        char *opening_quote = NULL;
        for (char *p = closing_quote - 1; p >= line; p--) {
            if (*p == '"') { opening_quote = p; break; }
        }
        if (!opening_quote || opening_quote >= closing_quote) continue;

        // Extract key name
        int klen = (int)(closing_quote - opening_quote - 1);
        if (klen <= 0 || klen >= MAX_KEY_LEN) continue;

        // Check if value after " = " is a number
        char *val_start = eq + 4; // skip `" = `
        while (*val_start == ' ') val_start++;

        char *endp;
        long long val = strtoll(val_start, &endp, 10);
        if (endp == val_start) continue; // not a number
        // Make sure we actually consumed something meaningful
        if (*endp != '\n' && *endp != '\r' && *endp != '\0' && *endp != ' ') continue;

        memcpy(kvs[count].name, opening_quote + 1, klen);
        kvs[count].name[klen] = '\0';
        kvs[count].value = (int64_t)val;
        count++;
    }

    pclose(fp);
    return count;
}

static int collect_sensor_noise(uint64_t *timings, int n) {
    int cap = n < SENSOR_LARGE_N ? n : SENSOR_LARGE_N;

    KeyVal *snap1 = (KeyVal *)malloc(MAX_KEYS * sizeof(KeyVal));
    KeyVal *snap2 = (KeyVal *)malloc(MAX_KEYS * sizeof(KeyVal));
    if (!snap1 || !snap2) { free(snap1); free(snap2); return 0; }

    int valid = 0;
    uint64_t prev_delta = 0;

    for (int round = 0; round < cap + 10 && valid < cap; round++) {
        int n1 = parse_ioreg(snap1, MAX_KEYS);
        usleep(50000); // 50ms delay
        int n2 = parse_ioreg(snap2, MAX_KEYS);

        // Find keys present in both snapshots with changed values
        for (int i = 0; i < n1 && valid < cap; i++) {
            for (int j = 0; j < n2; j++) {
                if (strcmp(snap1[i].name, snap2[j].name) == 0) {
                    int64_t delta = snap2[j].value - snap1[i].value;
                    if (delta != 0) {
                        uint64_t abs_delta = (uint64_t)(delta < 0 ? -delta : delta);
                        // XOR with previous delta
                        uint64_t xored = abs_delta ^ prev_delta;
                        timings[valid++] = xored;
                        prev_delta = abs_delta;
                    }
                    break;
                }
            }
        }
    }

    free(snap1);
    free(snap2);
    return valid;
}

// Cross-correlation: ioregistry — same ioreg mechanism with 4 snapshots
static int collect_ioregistry(uint64_t *timings, int n) {
    int cap = n < SENSOR_LARGE_N ? n : SENSOR_LARGE_N;

    KeyVal *snaps[4];
    int snap_counts[4];
    for (int s = 0; s < 4; s++) {
        snaps[s] = (KeyVal *)malloc(MAX_KEYS * sizeof(KeyVal));
        if (!snaps[s]) {
            for (int k = 0; k < s; k++) free(snaps[k]);
            return 0;
        }
    }

    int valid = 0;
    uint64_t prev_delta = 0;

    for (int round = 0; round < (cap / 4) + 5 && valid < cap; round++) {
        for (int s = 0; s < 4; s++) {
            snap_counts[s] = parse_ioreg(snaps[s], MAX_KEYS);
            if (s < 3) usleep(80000); // 80ms delay
        }

        // Find keys present in all 4 snapshots, compute deltas between consecutive
        for (int i = 0; i < snap_counts[0] && valid < cap; i++) {
            int found_all = 1;
            int64_t vals[4];
            vals[0] = snaps[0][i].value;

            for (int s = 1; s < 4 && found_all; s++) {
                int found = 0;
                for (int j = 0; j < snap_counts[s]; j++) {
                    if (strcmp(snaps[0][i].name, snaps[s][j].name) == 0) {
                        vals[s] = snaps[s][j].value;
                        found = 1;
                        break;
                    }
                }
                if (!found) found_all = 0;
            }

            if (!found_all) continue;

            for (int s = 1; s < 4 && valid < cap; s++) {
                int64_t delta = vals[s] - vals[s - 1];
                if (delta != 0) {
                    uint64_t abs_delta = (uint64_t)(delta < 0 ? -delta : delta);
                    uint64_t xored = abs_delta ^ prev_delta;
                    timings[valid++] = xored;
                    prev_delta = abs_delta;
                }
            }
        }
    }

    for (int s = 0; s < 4; s++) free(snaps[s]);
    return valid;
}

int main(void) {
    print_validation_header("sensor_noise");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    int large_n = SENSOR_LARGE_N;
    int trial_n = SENSOR_TRIAL_N;

    // === TEST 1: Sample entropy (capped) ===
    printf("=== Test 1: %d Sample Entropy (capped, ioreg is slow) ===\n", large_n);
    uint64_t *timings = (uint64_t *)malloc(large_n * sizeof(uint64_t));
    int valid = collect_sensor_noise(timings, large_n);

    if (valid < 100) {
        printf("  WARNING: Only got %d changing values (need >= 100)\n", valid);
        printf("  Automatic DEMOTE: insufficient changing ioreg keys\n\n");

        printf("=== SUMMARY ===\n");
        printf("  Samples: %d\n", valid);
        printf("  VERDICT: DEMOTE (fewer than 100 changing values)\n\n");
        free(timings);
        return 0;
    }

    Stats s = compute_stats(timings, valid);
    printf("  Samples: %d  Mean=%.1f  StdDev=%.1f\n", valid, s.mean, s.stddev);
    printf("  Shannon=%.3f  H_inf=%.3f\n\n", s.shannon, s.min_entropy);

    // === TEST 2: Autocorrelation (lag 1-5) ===
    printf("=== Test 2: Autocorrelation (lag 1-5) ===\n");
    double max_ac = 0;
    for (int lag = 1; lag <= 5; lag++) {
        double ac = autocorrelation(timings, valid, lag);
        printf("  lag-%d: %.4f%s\n", lag, ac,
               fabs(ac) > 0.5 ? " *** HIGH ***" : fabs(ac) > 0.1 ? " * warn *" : "");
        if (fabs(ac) > max_ac) max_ac = fabs(ac);
    }
    printf("\n");
    free(timings);

    // === TEST 3: 10 trials stability (capped) ===
    printf("=== Test 3: Stability (%d trials x %d samples) ===\n", N_TRIALS, trial_n);
    double min_ents[N_TRIALS];
    uint64_t *trial_t = (uint64_t *)malloc(trial_n * sizeof(uint64_t));
    for (int t = 0; t < N_TRIALS; t++) {
        int tv = collect_sensor_noise(trial_t, trial_n);
        Stats ts = compute_stats(trial_t, tv > 0 ? tv : 1);
        min_ents[t] = ts.min_entropy;
        printf("  Trial %2d: H_inf=%.3f  Shannon=%.3f  N=%d\n",
               t + 1, ts.min_entropy, ts.shannon, tv);
    }
    free(trial_t);

    double me_mean = 0, me_var = 0;
    for (int i = 0; i < N_TRIALS; i++) me_mean += min_ents[i];
    me_mean /= N_TRIALS;
    for (int i = 0; i < N_TRIALS; i++) {
        double d = min_ents[i] - me_mean;
        me_var += d * d;
    }
    double me_std = sqrt(me_var / N_TRIALS);
    printf("\n  H_inf Mean=%.3f  StdDev=%.3f\n", me_mean, me_std);
    printf("  Verdict: %s\n\n",
           me_std > 2.0 ? "UNSTABLE (std > 2.0)" :
           me_std > 1.0 ? "MARGINAL (std > 1.0)" : "STABLE");

    // === TEST 4: Cross-correlation ===
    printf("=== Test 4: Cross-correlation ===\n");
    int cc_n = 200; // small due to ioreg slowness
    uint64_t *my_t = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
    int my_v = collect_sensor_noise(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    if (cc_n > 10) {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_ioregistry(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        if (use > 0) {
            double r = pearson(my_t, other, use);
            printf("  vs %-25s: r=%.4f%s\n", "ioregistry", r,
                   fabs(r) > 0.3 ? " *** REDUNDANT ***" : fabs(r) > 0.1 ? " * weak *" : "");
        }
        free(other);
    } else {
        printf("  (skipped: insufficient samples for cross-correlation)\n");
    }
    free(my_t);
    printf("\n");

    // === SUMMARY ===
    printf("=== SUMMARY ===\n");
    printf("  H_inf (%d): %.3f\n", large_n, s.min_entropy);
    printf("  H_inf Mean (10 trials): %.3f\n", me_mean);
    printf("  H_inf StdDev: %.3f\n", me_std);
    printf("  Max autocorr: %.4f\n", max_ac);

    if (s.min_entropy < 0.5)
        printf("  VERDICT: CUT (H_inf < 0.5)\n");
    else if (me_std > 2.0)
        printf("  VERDICT: CUT (unstable, std > 2.0)\n");
    else if (s.min_entropy < 1.5 || max_ac > 0.5)
        printf("  VERDICT: DEMOTE (weak)\n");
    else
        printf("  VERDICT: KEEP\n");
    printf("\n");

    return 0;
}
