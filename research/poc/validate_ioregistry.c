// validate_ioregistry.c — IORegistry multi-snapshot delta entropy validation
// Mechanism: Run `ioreg -l -w0` 4 times with 80ms delays. Parse all numeric values.
//            Find keys present in all snapshots. Compute deltas for non-zero changes
//            across consecutive snapshots. XOR consecutive deltas, extract LSBs.
//            Slow source, capped at 500/200 samples.
// Cross-correlate with: sensor_noise (same ioreg mechanism)
// Compile: cc -O2 -o validate_ioregistry validate_ioregistry.c -lm

#include "validate_common.h"

#define IOREG_LARGE_N  500
#define IOREG_TRIAL_N  200
#define MAX_KEYS       8192
#define MAX_KEY_LEN    128

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
        char *closing_quote = eq;
        char *opening_quote = NULL;
        for (char *p = closing_quote - 1; p >= line; p--) {
            if (*p == '"') { opening_quote = p; break; }
        }
        if (!opening_quote || opening_quote >= closing_quote) continue;

        int klen = (int)(closing_quote - opening_quote - 1);
        if (klen <= 0 || klen >= MAX_KEY_LEN) continue;

        // Check if value after " = " is a number
        char *val_start = eq + 4;
        while (*val_start == ' ') val_start++;

        char *endp;
        long long val = strtoll(val_start, &endp, 10);
        if (endp == val_start) continue;
        if (*endp != '\n' && *endp != '\r' && *endp != '\0' && *endp != ' ') continue;

        memcpy(kvs[count].name, opening_quote + 1, klen);
        kvs[count].name[klen] = '\0';
        kvs[count].value = (int64_t)val;
        count++;
    }

    pclose(fp);
    return count;
}

// Find value of a key in a snapshot, return 1 if found
static int find_key(KeyVal *kvs, int nkvs, const char *name, int64_t *out_val) {
    for (int i = 0; i < nkvs; i++) {
        if (strcmp(kvs[i].name, name) == 0) {
            *out_val = kvs[i].value;
            return 1;
        }
    }
    return 0;
}

static int collect_ioregistry(uint64_t *timings, int n) {
    int cap = n < IOREG_LARGE_N ? n : IOREG_LARGE_N;

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

    for (int round = 0; round < (cap / 2) + 10 && valid < cap; round++) {
        // Take 4 snapshots with 80ms delays
        for (int s = 0; s < 4; s++) {
            snap_counts[s] = parse_ioreg(snaps[s], MAX_KEYS);
            if (s < 3) usleep(80000); // 80ms delay
        }

        // Find keys present in all 4 snapshots
        for (int i = 0; i < snap_counts[0] && valid < cap; i++) {
            int64_t vals[4];
            vals[0] = snaps[0][i].value;
            int found_all = 1;

            for (int s = 1; s < 4 && found_all; s++) {
                if (!find_key(snaps[s], snap_counts[s], snaps[0][i].name, &vals[s])) {
                    found_all = 0;
                }
            }
            if (!found_all) continue;

            // Compute deltas between consecutive snapshots
            for (int s = 1; s < 4 && valid < cap; s++) {
                int64_t delta = vals[s] - vals[s - 1];
                if (delta != 0) {
                    uint64_t abs_delta = (uint64_t)(delta < 0 ? -delta : delta);
                    // XOR with previous delta, extract LSBs
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

// Cross-correlation: sensor_noise — 2-snapshot ioreg with 50ms delay
static int collect_sensor_noise(uint64_t *timings, int n) {
    int cap = n < IOREG_LARGE_N ? n : IOREG_LARGE_N;

    KeyVal *snap1 = (KeyVal *)malloc(MAX_KEYS * sizeof(KeyVal));
    KeyVal *snap2 = (KeyVal *)malloc(MAX_KEYS * sizeof(KeyVal));
    if (!snap1 || !snap2) { free(snap1); free(snap2); return 0; }

    int valid = 0;
    uint64_t prev_delta = 0;

    for (int round = 0; round < cap + 10 && valid < cap; round++) {
        int n1 = parse_ioreg(snap1, MAX_KEYS);
        usleep(50000); // 50ms delay
        int n2 = parse_ioreg(snap2, MAX_KEYS);

        for (int i = 0; i < n1 && valid < cap; i++) {
            for (int j = 0; j < n2; j++) {
                if (strcmp(snap1[i].name, snap2[j].name) == 0) {
                    int64_t delta = snap2[j].value - snap1[i].value;
                    if (delta != 0) {
                        uint64_t abs_delta = (uint64_t)(delta < 0 ? -delta : delta);
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

int main(void) {
    print_validation_header("ioregistry");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    int large_n = IOREG_LARGE_N;
    int trial_n = IOREG_TRIAL_N;

    // === TEST 1: Sample entropy (capped) ===
    printf("=== Test 1: %d Sample Entropy (capped, ioreg is slow) ===\n", large_n);
    uint64_t *timings = (uint64_t *)malloc(large_n * sizeof(uint64_t));
    int valid = collect_ioregistry(timings, large_n);

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
        int tv = collect_ioregistry(trial_t, trial_n);
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
    int cc_n = 200;
    uint64_t *my_t = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
    int my_v = collect_ioregistry(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    if (cc_n > 10) {
        uint64_t *other = (uint64_t *)malloc(cc_n * sizeof(uint64_t));
        int ov = collect_sensor_noise(other, cc_n);
        int use = cc_n < ov ? cc_n : ov;
        if (use > 0) {
            double r = pearson(my_t, other, use);
            printf("  vs %-25s: r=%.4f%s\n", "sensor_noise", r,
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
