// Filesystem Journal Commit Timing — Full storage stack entropy
//
// APFS uses copy-on-write with a journal. Each fsync crosses:
//   CPU → filesystem → NVMe controller → NAND flash → back
//
// Each layer adds independent noise:
// - Checksum computation (CPU pipeline state)
// - NVMe command queuing and arbitration
// - Flash cell program/erase timing (temperature-dependent)
// - B-tree update (memory allocation nondeterminism)
// - Barrier flush (controller firmware scheduling)
//
// Different from disk_io because this specifically measures the full
// journal commit path, not just raw block reads.
//
// Build: cc -O2 -o unprecedented_fsync_journal unprecedented_fsync_journal.c -lm

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/stat.h>
#include <mach/mach_time.h>

#define N_SAMPLES 12000
#define WRITE_SIZES_COUNT 4

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
    printf("# Filesystem Journal Commit Timing — Full Storage Stack Entropy\n\n");

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);
    double ns_per_tick = (double)tb.numer / tb.denom;

    // Create temp directory for test files
    char tmpdir[256];
    snprintf(tmpdir, sizeof(tmpdir), "/tmp/fsync_entropy_XXXXXX");
    if (!mkdtemp(tmpdir)) {
        perror("mkdtemp");
        return 1;
    }
    printf("Test directory: %s\n\n", tmpdir);

    uint64_t *timings = malloc(N_SAMPLES * sizeof(uint64_t));
    uint8_t *lsbs = malloc(N_SAMPLES);
    uint8_t *write_buf = malloc(4096);
    memset(write_buf, 0xAA, 4096);

    // === Test 1: Single file create+write+fsync cycle ===
    printf("--- Test 1: Create+Write+Fsync Cycle ---\n");
    int write_sizes[WRITE_SIZES_COUNT] = {64, 512, 1024, 4096};

    for (int ws = 0; ws < WRITE_SIZES_COUNT; ws++) {
        int wsize = write_sizes[ws];
        printf("\n  Write size: %d bytes\n", wsize);

        for (int i = 0; i < N_SAMPLES; i++) {
            char path[512];
            snprintf(path, sizeof(path), "%s/f_%d_%d", tmpdir, ws, i);

            uint64_t t0 = mach_absolute_time();
            int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
            if (fd < 0) { timings[i] = 0; continue; }

            // Write data with varying content (affects APFS compression decisions)
            write_buf[0] = (uint8_t)(i & 0xFF);
            write_buf[1] = (uint8_t)((i >> 8) & 0xFF);
            write(fd, write_buf, wsize);
            fsync(fd);
            close(fd);
            uint64_t t1 = mach_absolute_time();

            timings[i] = t1 - t0;
            lsbs[i] = (uint8_t)(timings[i] & 0xFF);

            // Cleanup every 100 files to avoid filling up tmpfs
            if (i % 100 == 99) {
                for (int j = i - 99; j <= i; j++) {
                    snprintf(path, sizeof(path), "%s/f_%d_%d", tmpdir, ws, j);
                    unlink(path);
                }
            }
        }

        // Cleanup remaining files
        for (int i = (N_SAMPLES / 100) * 100; i < N_SAMPLES; i++) {
            char path[512];
            snprintf(path, sizeof(path), "%s/f_%d_%d", tmpdir, ws, i);
            unlink(path);
        }

        uint64_t tmin = timings[0], tmax = timings[0], tsum = 0;
        for (int i = 0; i < N_SAMPLES; i++) {
            if (timings[i] < tmin) tmin = timings[i];
            if (timings[i] > tmax) tmax = timings[i];
            tsum += timings[i];
        }
        printf("  Timing range: %llu - %llu ticks (%.0f - %.0f µs), mean=%llu\n",
               tmin, tmax, tmin * ns_per_tick / 1000, tmax * ns_per_tick / 1000,
               tsum/N_SAMPLES);

        char label[64];
        snprintf(label, sizeof(label), "Fsync %dB LSBs", wsize);
        analyze_entropy(label, lsbs, N_SAMPLES);
    }

    // === Test 2: Overwrite-in-place fsync ===
    printf("\n--- Test 2: Overwrite-in-Place Fsync ---\n");
    {
        char path[512];
        snprintf(path, sizeof(path), "%s/overwrite_test", tmpdir);
        int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
        write(fd, write_buf, 4096);
        fsync(fd);
        close(fd);

        for (int i = 0; i < N_SAMPLES; i++) {
            fd = open(path, O_WRONLY, 0644);
            if (fd < 0) { timings[i] = 0; continue; }

            // Vary write position within file
            lseek(fd, i % 4096, SEEK_SET);
            write_buf[0] = (uint8_t)i;

            uint64_t t0 = mach_absolute_time();
            write(fd, write_buf, 64);
            fsync(fd);
            uint64_t t1 = mach_absolute_time();
            close(fd);

            timings[i] = t1 - t0;
            lsbs[i] = (uint8_t)(timings[i] & 0xFF);
        }

        uint64_t tmin = timings[0], tmax = timings[0], tsum = 0;
        for (int i = 0; i < N_SAMPLES; i++) {
            if (timings[i] < tmin) tmin = timings[i];
            if (timings[i] > tmax) tmax = timings[i];
            tsum += timings[i];
        }
        printf("  Timing range: %llu - %llu ticks (%.0f - %.0f µs), mean=%llu\n",
               tmin, tmax, tmin * ns_per_tick / 1000, tmax * ns_per_tick / 1000,
               tsum/N_SAMPLES);
        analyze_entropy("Overwrite fsync LSBs", lsbs, N_SAMPLES);

        unlink(path);
    }

    // === Test 3: Delta timing ===
    printf("\n--- Test 3: Fsync Delta Timing ---\n");
    {
        char path[512];
        snprintf(path, sizeof(path), "%s/delta_test", tmpdir);

        for (int i = 0; i < N_SAMPLES; i++) {
            int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
            if (fd < 0) { timings[i] = 0; continue; }
            write_buf[0] = (uint8_t)i;
            write(fd, write_buf, 512);
            uint64_t t0 = mach_absolute_time();
            fsync(fd);
            uint64_t t1 = mach_absolute_time();
            close(fd);
            timings[i] = t1 - t0;
        }
        unlink(path);

        uint8_t *deltas = malloc(N_SAMPLES);
        for (int i = 1; i < N_SAMPLES; i++) {
            int64_t d = (int64_t)timings[i] - (int64_t)timings[i-1];
            uint64_t ud = (uint64_t)d;
            deltas[i-1] = (uint8_t)((ud >> 0) ^ (ud >> 8) ^ (ud >> 16) ^ (ud >> 24));
        }
        analyze_entropy("Fsync delta XOR-fold", deltas, N_SAMPLES - 1);
        free(deltas);
    }

    // === Test 4: Multiple file fsync (B-tree churn) ===
    printf("\n--- Test 4: Multi-File Fsync (B-tree Churn) ---\n");
    {
        // Create many small files to stress APFS B-tree
        for (int i = 0; i < N_SAMPLES; i++) {
            char path[512];
            snprintf(path, sizeof(path), "%s/btree_%d", tmpdir, i % 50);

            uint64_t t0 = mach_absolute_time();
            int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
            if (fd >= 0) {
                write_buf[0] = (uint8_t)i;
                write_buf[1] = (uint8_t)(i >> 8);
                write(fd, write_buf, 128);
                fsync(fd);
                close(fd);
            }
            uint64_t t1 = mach_absolute_time();
            timings[i] = t1 - t0;
            lsbs[i] = (uint8_t)(timings[i] & 0xFF);
        }

        // Cleanup
        for (int i = 0; i < 50; i++) {
            char path[512];
            snprintf(path, sizeof(path), "%s/btree_%d", tmpdir, i);
            unlink(path);
        }

        uint64_t tmin = timings[0], tmax = timings[0], tsum = 0;
        for (int i = 0; i < N_SAMPLES; i++) {
            if (timings[i] < tmin) tmin = timings[i];
            if (timings[i] > tmax) tmax = timings[i];
            tsum += timings[i];
        }
        printf("  Timing range: %llu - %llu ticks (%.0f - %.0f µs), mean=%llu\n",
               tmin, tmax, tmin * ns_per_tick / 1000, tmax * ns_per_tick / 1000,
               tsum/N_SAMPLES);
        analyze_entropy("Multi-file fsync LSBs", lsbs, N_SAMPLES);
    }

    rmdir(tmpdir);
    free(timings);
    free(lsbs);
    free(write_buf);

    printf("\nDone.\n");
    return 0;
}
