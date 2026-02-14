// validate_spotlight_timing.c — Entropy source validation
// Mechanism: Run mdls on system files, measure process spawn+completion time
// Note: Capped at 200 iterations per collection to keep runtime reasonable
// Cross-correlate: dyld_timing, ioregistry
// Compile: cc -O2 -o validate_spotlight_timing validate_spotlight_timing.c -lm

#include "validate_common.h"
#include <sys/wait.h>
#include <signal.h>

#define SPOT_CAP 200
#define SPOT_TRIAL_CAP 200
#define SPOT_TRIAL_N (TRIAL_N < SPOT_TRIAL_CAP ? TRIAL_N : SPOT_TRIAL_CAP)
#define SPOT_LARGE_N (LARGE_N < SPOT_CAP ? LARGE_N : SPOT_CAP)

static const char *g_target_files[] = {
    "/usr/bin/true",
    "/usr/bin/false",
    "/usr/bin/env",
    "/usr/bin/id",
    "/usr/bin/who",
    "/usr/bin/wc",
    "/usr/bin/sort",
    "/usr/bin/head",
};
static const int g_ntargets = 8;

static int collect_spotlight_timing(uint64_t *timings, int n) {
    int valid = 0;
    int devnull = open("/dev/null", O_WRONLY);

    for (int i = 0; i < n; i++) {
        const char *target = g_target_files[i % g_ntargets];

        uint64_t t0 = mach_absolute_time();
        pid_t pid = fork();
        if (pid == 0) {
            // Child: redirect stdout/stderr to /dev/null
            if (devnull >= 0) {
                dup2(devnull, STDOUT_FILENO);
                dup2(devnull, STDERR_FILENO);
            }
            execl("/usr/bin/mdls", "mdls", "-name", "kMDItemFSName", target, NULL);
            _exit(127);
        } else if (pid > 0) {
            int status;
            waitpid(pid, &status, 0);
            uint64_t t1 = mach_absolute_time();
            timings[valid++] = t1 - t0;
        }
    }

    if (devnull >= 0) close(devnull);
    return valid;
}

// Cross-correlation: dyld_timing (process/filesystem interaction)
static int collect_dyld_cross(uint64_t *timings, int n) {
    int valid = 0;
    int devnull = open("/dev/null", O_WRONLY);

    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();
        pid_t pid = fork();
        if (pid == 0) {
            if (devnull >= 0) {
                dup2(devnull, STDOUT_FILENO);
                dup2(devnull, STDERR_FILENO);
            }
            execl("/usr/bin/true", "true", NULL);
            _exit(127);
        } else if (pid > 0) {
            int status;
            waitpid(pid, &status, 0);
            uint64_t t1 = mach_absolute_time();
            timings[valid++] = t1 - t0;
        }
    }

    if (devnull >= 0) close(devnull);
    return valid;
}

// Cross-correlation: ioregistry (system command timing)
static int collect_ioregistry_cross(uint64_t *timings, int n) {
    int valid = 0;
    int devnull = open("/dev/null", O_WRONLY);

    for (int i = 0; i < n; i++) {
        uint64_t t0 = mach_absolute_time();
        pid_t pid = fork();
        if (pid == 0) {
            if (devnull >= 0) {
                dup2(devnull, STDOUT_FILENO);
                dup2(devnull, STDERR_FILENO);
            }
            execl("/usr/sbin/ioreg", "ioreg", "-c", "IOPlatformExpertDevice", "-d", "1", NULL);
            _exit(127);
        } else if (pid > 0) {
            int status;
            waitpid(pid, &status, 0);
            uint64_t t1 = mach_absolute_time();
            timings[valid++] = t1 - t0;
        }
    }

    if (devnull >= 0) close(devnull);
    return valid;
}

int main(void) {
    print_validation_header("spotlight_timing");
    printf("  NOTE: Capped at %d samples per collection (process spawn is slow)\n\n", SPOT_CAP);

    // === Test 1: Large sample entropy (capped) ===
    printf("=== Test 1: %d Sample Entropy ===\n", SPOT_LARGE_N);
    uint64_t *timings = malloc(SPOT_LARGE_N * sizeof(uint64_t));
    int valid = collect_spotlight_timing(timings, SPOT_LARGE_N);
    Stats s = compute_stats(timings, valid);
    printf("  Samples: %d  Mean=%.1f  StdDev=%.1f\n", valid, s.mean, s.stddev);
    printf("  Shannon=%.3f  H_inf=%.3f\n\n", s.shannon, s.min_entropy);

    // === Test 2: Autocorrelation ===
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

    // === Test 3: Stability (capped trials) ===
    int trial_n = SPOT_TRIAL_N;
    printf("=== Test 3: Stability (%d trials x %d samples) ===\n", N_TRIALS, trial_n);
    double min_ents[N_TRIALS];
    uint64_t *trial_buf = malloc(trial_n * sizeof(uint64_t));
    for (int t = 0; t < N_TRIALS; t++) {
        int tv = collect_spotlight_timing(trial_buf, trial_n);
        Stats ts = compute_stats(trial_buf, tv > 0 ? tv : 1);
        min_ents[t] = ts.min_entropy;
        printf("  Trial %2d: H_inf=%.3f  Shannon=%.3f  N=%d\n",
               t + 1, ts.min_entropy, ts.shannon, tv);
    }
    free(trial_buf);

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

    // === Test 4: Cross-correlation (smaller N due to process spawning) ===
    int cc_n = 100; // Keep small for process-spawning sources
    printf("=== Test 4: Cross-correlation (N=%d) ===\n", cc_n);
    uint64_t *my_t = malloc(cc_n * sizeof(uint64_t));
    int my_v = collect_spotlight_timing(my_t, cc_n);
    if (my_v < cc_n) cc_n = my_v;

    const char *cross_names[] = {"dyld_timing", "ioregistry"};
    collect_func_t cross_funcs[] = {collect_dyld_cross, collect_ioregistry_cross};
    for (int c = 0; c < 2; c++) {
        uint64_t *other_t = malloc(cc_n * sizeof(uint64_t));
        int other_v = cross_funcs[c](other_t, cc_n);
        int use_n = cc_n < other_v ? cc_n : other_v;
        double r = pearson(my_t, other_t, use_n);
        printf("  vs %-25s: r=%.4f%s\n", cross_names[c], r,
               fabs(r) > 0.3 ? " *** REDUNDANT ***" :
               fabs(r) > 0.1 ? " * weak *" : "");
        free(other_t);
    }
    free(my_t);
    printf("\n");

    // === Summary ===
    printf("=== SUMMARY ===\n");
    printf("  H_inf (%d): %.3f\n", SPOT_LARGE_N, s.min_entropy);
    printf("  H_inf Mean (%d trials): %.3f\n", N_TRIALS, me_mean);
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
