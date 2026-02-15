// NVMe Flash Cell Read Latency — NAND physics entropy
//
// NVMe SSDs have NAND flash cells whose read latency depends on:
// - Charge state of neighboring cells (cross-coupling)
// - Number of program/erase cycles (oxide wear)
// - Temperature-dependent charge retention
// - Read disturb effects from prior reads
// - SSD internal garbage collection nondeterminism
//
// By reading the SAME sector repeatedly with nanosecond timing, we capture
// flash cell physics variance distinct from filesystem caching.
//
// Build: cc -O2 -o unprecedented_nvme_latency unprecedented_nvme_latency.c
// Note: Uses /dev/rdisk0 (raw device) for bypass of filesystem cache.
//       May need root for raw disk access.

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/stat.h>
#include <mach/mach_time.h>

#define N_SAMPLES 15000
#define BLOCK_SIZE 4096
#define N_OFFSETS  8

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
    printf("# NVMe Flash Cell Read Latency — NAND Physics Entropy\n\n");

    // Try multiple approaches: raw disk, then temp file
    int fd = -1;
    const char *device = NULL;

    // Try raw disk first (needs root)
    const char *devices[] = {"/dev/rdisk0", "/dev/rdisk1", NULL};
    for (int i = 0; devices[i]; i++) {
        fd = open(devices[i], O_RDONLY);
        if (fd >= 0) {
            device = devices[i];
            break;
        }
    }

    // Fallback: use a temp file with F_NOCACHE to bypass buffer cache
    char tmppath[256] = "/tmp/nvme_entropy_probe_XXXXXX";
    int using_tmpfile = 0;
    if (fd < 0) {
        fd = mkstemp(tmppath);
        if (fd < 0) {
            perror("Cannot open any device or create temp file");
            return 1;
        }
        device = tmppath;
        using_tmpfile = 1;

        // Write test data — different patterns to hit different NAND states
        uint8_t *buf = malloc(BLOCK_SIZE * N_OFFSETS);
        for (int i = 0; i < BLOCK_SIZE * N_OFFSETS; i++) {
            buf[i] = (uint8_t)((i * 7 + 13) ^ (i >> 8));
        }
        write(fd, buf, BLOCK_SIZE * N_OFFSETS);
        fsync(fd);
        free(buf);

        // Disable buffer caching (macOS-specific)
        fcntl(fd, F_NOCACHE, 1);

        printf("Using temp file: %s (with F_NOCACHE)\n", tmppath);
    } else {
        printf("Using raw device: %s\n", device);
        fcntl(fd, F_NOCACHE, 1);
    }

    uint8_t *read_buf = (uint8_t *)malloc(BLOCK_SIZE);
    if (!read_buf) { close(fd); return 1; }

    // Aligned read buffer for direct I/O
    void *aligned_buf = NULL;
    posix_memalign(&aligned_buf, BLOCK_SIZE, BLOCK_SIZE);
    if (!aligned_buf) { free(read_buf); close(fd); return 1; }

    // Offsets to read from (different NAND pages/dies)
    off_t offsets[N_OFFSETS];
    for (int i = 0; i < N_OFFSETS; i++) {
        offsets[i] = (off_t)i * BLOCK_SIZE;
    }

    mach_timebase_info_data_t tb;
    mach_timebase_info(&tb);

    // === Test 1: Same-sector repeated read timing ===
    printf("\n--- Test 1: Same-Sector Repeated Read Timing ---\n");
    uint64_t *timings = malloc(N_SAMPLES * sizeof(uint64_t));
    uint8_t *lsbs = malloc(N_SAMPLES);

    for (int i = 0; i < N_SAMPLES; i++) {
        lseek(fd, offsets[0], SEEK_SET);
        uint64_t t0 = mach_absolute_time();
        ssize_t r = read(fd, aligned_buf, BLOCK_SIZE);
        uint64_t t1 = mach_absolute_time();
        (void)r;
        timings[i] = t1 - t0;
        lsbs[i] = (uint8_t)(timings[i] & 0xFF);
    }

    // Show timing stats
    uint64_t tmin = timings[0], tmax = timings[0], tsum = 0;
    for (int i = 0; i < N_SAMPLES; i++) {
        if (timings[i] < tmin) tmin = timings[i];
        if (timings[i] > tmax) tmax = timings[i];
        tsum += timings[i];
    }
    printf("  Timing range: %llu - %llu ticks, mean=%llu\n",
           tmin, tmax, tsum / N_SAMPLES);
    analyze_entropy("Same-sector LSBs", lsbs, N_SAMPLES);

    // === Test 2: Multi-offset read timing ===
    printf("\n--- Test 2: Multi-Offset Read Timing ---\n");
    for (int i = 0; i < N_SAMPLES; i++) {
        off_t offset = offsets[i % N_OFFSETS];
        lseek(fd, offset, SEEK_SET);
        uint64_t t0 = mach_absolute_time();
        ssize_t r = read(fd, aligned_buf, BLOCK_SIZE);
        uint64_t t1 = mach_absolute_time();
        (void)r;
        timings[i] = t1 - t0;
        lsbs[i] = (uint8_t)(timings[i] & 0xFF);
    }
    analyze_entropy("Multi-offset LSBs", lsbs, N_SAMPLES);

    // === Test 3: Delta timing (consecutive read jitter) ===
    printf("\n--- Test 3: Delta Timing ---\n");
    uint8_t *deltas = malloc(N_SAMPLES);
    for (int i = 1; i < N_SAMPLES; i++) {
        int64_t d = (int64_t)timings[i] - (int64_t)timings[i-1];
        // XOR-fold the delta to a byte
        uint64_t ud = (uint64_t)d;
        deltas[i-1] = (uint8_t)((ud >> 0) ^ (ud >> 8) ^ (ud >> 16) ^ (ud >> 24));
    }
    analyze_entropy("Delta XOR-fold", deltas, N_SAMPLES - 1);

    // === Test 4: Read-after-write timing (WAF and GC effects) ===
    printf("\n--- Test 4: Read-After-Write Timing ---\n");
    if (using_tmpfile) {
        uint8_t write_buf[512];
        for (int i = 0; i < N_SAMPLES; i++) {
            // Write a small amount to trigger NVMe activity
            memset(write_buf, (uint8_t)i, sizeof(write_buf));
            lseek(fd, BLOCK_SIZE * N_OFFSETS + (i % 8) * 512, SEEK_SET);
            write(fd, write_buf, sizeof(write_buf));
            // Don't fsync every time — let writes batch

            // Now read from a different offset
            lseek(fd, offsets[i % N_OFFSETS], SEEK_SET);
            uint64_t t0 = mach_absolute_time();
            ssize_t r = read(fd, aligned_buf, BLOCK_SIZE);
            uint64_t t1 = mach_absolute_time();
            (void)r;
            timings[i] = t1 - t0;
            lsbs[i] = (uint8_t)(timings[i] & 0xFF);
        }
        analyze_entropy("Read-after-write LSBs", lsbs, N_SAMPLES);
    } else {
        printf("  Skipped (read-only device)\n");
    }

    // === Test 5: Burst read timing (back-to-back reads) ===
    printf("\n--- Test 5: Burst Read Timing ---\n");
    for (int i = 0; i < N_SAMPLES; i++) {
        lseek(fd, offsets[i % N_OFFSETS], SEEK_SET);
        uint64_t t0 = mach_absolute_time();
        // Multiple reads back-to-back to stress the NVMe queue
        for (int j = 0; j < 4; j++) {
            read(fd, aligned_buf, BLOCK_SIZE);
        }
        uint64_t t1 = mach_absolute_time();
        timings[i] = t1 - t0;
        // Use bits 2-9 (skip 2 LSBs for more entropy)
        lsbs[i] = (uint8_t)((timings[i] >> 2) & 0xFF);
    }
    analyze_entropy("Burst read bits[2:9]", lsbs, N_SAMPLES);

    free(timings);
    free(lsbs);
    free(deltas);
    free(aligned_buf);
    close(fd);
    if (using_tmpfile) unlink(tmppath);

    printf("\nDone.\n");
    return 0;
}
