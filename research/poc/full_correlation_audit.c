/*
 * full_correlation_audit.c — Cross-correlation analysis of frontier entropy sources.
 *
 * This program collects timing samples from multiple entropy-generating operations
 * and computes the Pearson correlation coefficient between every pair.
 * Any pair with |r| > 0.15 is flagged as potentially redundant.
 *
 * NOTE: This C program tests representative TIMING patterns from each source.
 * For a full correlation test using the actual Rust source implementations,
 * use the Rust integration test in crates/openentropy-tests/ instead.
 *
 * Build: clang -O2 -framework IOKit -framework CoreFoundation -framework Security \
 *        -framework CoreAudio -framework Accelerate -framework Metal \
 *        -o full_correlation_audit full_correlation_audit.c -lm
 *
 * Run: ./full_correlation_audit
 */

#include <fcntl.h>
#include <math.h>
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/event.h>
#include <sys/mman.h>
#include <sys/types.h>
#include <unistd.h>

#define N_SAMPLES 10000
#define N_SOURCES 14  /* Number of non-hardware-dependent sources we can test in C */

/* Source names for the correlation matrix output. */
static const char *SOURCE_NAMES[N_SOURCES] = {
    "thread_lifecycle",
    "mach_ipc",
    "tlb_shootdown",
    "pipe_buffer",
    "kqueue_events",
    "dvfs_race",
    "cas_contention",
    "denormal_timing",
    "fsync_journal",
    "nvme_latency",
    "pdn_resonance",
    "amx_timing",
    "mach_timing",     /* baseline: existing source */
    "clock_jitter",    /* baseline: existing source */
};

static double samples[N_SOURCES][N_SAMPLES];

/*
 * Collect timing samples for each source.
 * Each function stores `N_SAMPLES` timing deltas in the `samples` array.
 */

/* 0: thread_lifecycle — spawn+join timing */
static void collect_thread_lifecycle(void) {
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        pthread_t th;
        pthread_create(&th, NULL, (void *(*)(void *))pthread_exit, NULL);
        pthread_join(th, NULL);
        uint64_t t1 = mach_absolute_time();
        samples[0][i] = (double)(t1 - t0);
    }
}

/* 1: mach_ipc — port allocate/deallocate timing */
static void collect_mach_ipc(void) {
    mach_port_t task = mach_task_self();
    for (int i = 0; i < N_SAMPLES; i++) {
        mach_port_t port;
        uint64_t t0 = mach_absolute_time();
        mach_port_allocate(task, MACH_PORT_RIGHT_RECEIVE, &port);
        mach_port_deallocate(task, port);
        mach_port_mod_refs(task, port, MACH_PORT_RIGHT_RECEIVE, -1);
        uint64_t t1 = mach_absolute_time();
        samples[1][i] = (double)(t1 - t0);
    }
}

/* 2: tlb_shootdown — mprotect timing */
static void collect_tlb_shootdown(void) {
    size_t page_size = sysconf(_SC_PAGESIZE);
    size_t region_size = page_size * 256;
    void *addr = mmap(NULL, region_size, PROT_READ | PROT_WRITE,
                      MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
    if (addr == MAP_FAILED) return;

    /* Touch all pages. */
    for (size_t p = 0; p < 256; p++)
        ((volatile char *)addr)[p * page_size] = 0xAA;

    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        mprotect(addr, region_size, PROT_READ);
        mprotect(addr, region_size, PROT_READ | PROT_WRITE);
        uint64_t t1 = mach_absolute_time();
        samples[2][i] = (double)(t1 - t0);
    }
    munmap(addr, region_size);
}

/* 3: pipe_buffer — pipe write+read timing */
static void collect_pipe_buffer(void) {
    for (int i = 0; i < N_SAMPLES; i++) {
        int fds[2];
        uint64_t t0 = mach_absolute_time();
        pipe(fds);
        char buf[64] = {0};
        write(fds[1], buf, sizeof(buf));
        read(fds[0], buf, sizeof(buf));
        close(fds[0]);
        close(fds[1]);
        uint64_t t1 = mach_absolute_time();
        samples[3][i] = (double)(t1 - t0);
    }
}

/* 4: kqueue_events — kevent with timer timing */
static void collect_kqueue_events(void) {
    int kq = kqueue();
    if (kq < 0) return;

    struct kevent ev;
    EV_SET(&ev, 1, EVFILT_TIMER, EV_ADD | EV_ENABLE, 0, 1, NULL);
    kevent(kq, &ev, 1, NULL, 0, NULL);

    struct timespec ts = {0, 1000000}; /* 1ms timeout */

    for (int i = 0; i < N_SAMPLES; i++) {
        struct kevent out;
        uint64_t t0 = mach_absolute_time();
        kevent(kq, NULL, 0, &out, 1, &ts);
        uint64_t t1 = mach_absolute_time();
        samples[4][i] = (double)(t1 - t0);
    }
    close(kq);
}

/* 5: dvfs_race — two-thread race counting */
static volatile int dvfs_stop;
static volatile uint64_t dvfs_count1, dvfs_count2;

static void *dvfs_racer(void *arg) {
    uint64_t count = 0;
    while (!dvfs_stop) count++;
    *(volatile uint64_t *)arg = count;
    return NULL;
}

static void collect_dvfs_race(void) {
    for (int i = 0; i < N_SAMPLES; i++) {
        dvfs_stop = 0;
        dvfs_count1 = dvfs_count2 = 0;
        pthread_t t1, t2;
        pthread_create(&t1, NULL, dvfs_racer, (void *)&dvfs_count1);
        pthread_create(&t2, NULL, dvfs_racer, (void *)&dvfs_count2);

        /* ~2μs race window */
        uint64_t start = mach_absolute_time();
        while (mach_absolute_time() - start < 48)
            ;
        dvfs_stop = 1;
        pthread_join(t1, NULL);
        pthread_join(t2, NULL);

        uint64_t diff = dvfs_count1 > dvfs_count2
                            ? dvfs_count1 - dvfs_count2
                            : dvfs_count2 - dvfs_count1;
        samples[5][i] = (double)diff;
    }
}

/* 6: cas_contention — atomic CAS timing */
static atomic_uint_fast64_t cas_target;

static void collect_cas_contention(void) {
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        uint64_t expected = atomic_load_explicit(&cas_target, memory_order_relaxed);
        atomic_compare_exchange_weak_explicit(&cas_target, &expected, expected + 1,
                                              memory_order_acq_rel, memory_order_relaxed);
        uint64_t t1 = mach_absolute_time();
        samples[6][i] = (double)(t1 - t0);
    }
}

/* 7: denormal_timing — FPU denormal operation timing */
static void collect_denormal_timing(void) {
    for (int i = 0; i < N_SAMPLES; i++) {
        double acc = 5e-324; /* smallest denormal */
        uint64_t t0 = mach_absolute_time();
        for (int j = 0; j < 100; j++) {
            acc *= 5e-324;
            acc += 5e-324;
        }
        uint64_t t1 = mach_absolute_time();
        *(volatile double *)&acc; /* prevent optimization */
        samples[7][i] = (double)(t1 - t0);
    }
}

/* 8: fsync_journal — file write+fsync timing */
static void collect_fsync_journal(void) {
    for (int i = 0; i < N_SAMPLES; i++) {
        char path[256];
        snprintf(path, sizeof(path), "/tmp/oe_corr_%d_%d", getpid(), i);
        int fd = open(path, O_CREAT | O_WRONLY | O_TRUNC, 0600);
        if (fd < 0) continue;
        char buf[512] = {0};
        buf[0] = i & 0xFF;
        uint64_t t0 = mach_absolute_time();
        write(fd, buf, sizeof(buf));
        fsync(fd);
        uint64_t t1 = mach_absolute_time();
        close(fd);
        unlink(path);
        samples[8][i] = (double)(t1 - t0);
    }
}

/* 9: nvme_latency — file read with F_NOCACHE timing */
static void collect_nvme_latency(void) {
    char path[256];
    snprintf(path, sizeof(path), "/tmp/oe_nvme_%d", getpid());
    int fd = open(path, O_CREAT | O_RDWR | O_TRUNC, 0600);
    if (fd < 0) return;

    char buf[32768];
    memset(buf, 0xAB, sizeof(buf));
    write(fd, buf, sizeof(buf));
    fsync(fd);
    fcntl(fd, F_NOCACHE, 1);

    for (int i = 0; i < N_SAMPLES; i++) {
        off_t offset = (i % 8) * 4096;
        lseek(fd, offset, SEEK_SET);
        char rbuf[4096];
        uint64_t t0 = mach_absolute_time();
        read(fd, rbuf, sizeof(rbuf));
        uint64_t t1 = mach_absolute_time();
        samples[9][i] = (double)(t1 - t0);
    }
    close(fd);
    unlink(path);
}

/* 10: pdn_resonance — timing with stress threads */
static volatile int pdn_stop;

static void *pdn_stress_mem(void *arg) {
    uint64_t *buf = malloc(4 * 1024 * 1024);
    while (!pdn_stop) {
        for (size_t i = 0; i < 512 * 1024; i += 64)
            buf[i]++;
    }
    free(buf);
    return NULL;
}

static void collect_pdn_resonance(void) {
    pdn_stop = 0;
    pthread_t stress;
    pthread_create(&stress, NULL, pdn_stress_mem, NULL);
    usleep(1000); /* warmup */

    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        volatile uint64_t acc = 0;
        for (int j = 0; j < 100; j++) acc += j;
        uint64_t t1 = mach_absolute_time();
        samples[10][i] = (double)(t1 - t0);
    }
    pdn_stop = 1;
    pthread_join(stress, NULL);
}

/* 11: amx_timing — cblas_sgemm timing (Accelerate framework) */
/* Note: linking Accelerate would be needed; use a proxy FP workload. */
static void collect_amx_timing(void) {
    /* Matrix multiply proxy using plain C (approximates AMX path). */
    float a[64 * 64], b[64 * 64], c[64 * 64];
    for (int k = 0; k < 64 * 64; k++) {
        a[k] = sinf(k * 0.01f);
        b[k] = cosf(k * 0.01f);
    }

    for (int i = 0; i < N_SAMPLES; i++) {
        memset(c, 0, sizeof(c));
        uint64_t t0 = mach_absolute_time();
        /* Simple matmul — not going through Accelerate in this C test,
         * but exercises similar FPU/SIMD path. */
        for (int r = 0; r < 64; r++)
            for (int col = 0; col < 64; col++) {
                float sum = 0;
                for (int k = 0; k < 64; k++)
                    sum += a[r * 64 + k] * b[k * 64 + col];
                c[r * 64 + col] = sum;
            }
        uint64_t t1 = mach_absolute_time();
        *(volatile float *)&c[0];
        samples[11][i] = (double)(t1 - t0);
    }
}

/* 12: mach_timing — baseline: just mach_absolute_time differences */
static void collect_mach_timing(void) {
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        /* Minimal work. */
        for (volatile int j = 0; j < 10; j++)
            ;
        uint64_t t1 = mach_absolute_time();
        samples[12][i] = (double)(t1 - t0);
    }
}

/* 13: clock_jitter — baseline: consecutive timestamp differences */
static void collect_clock_jitter(void) {
    for (int i = 0; i < N_SAMPLES; i++) {
        uint64_t t0 = mach_absolute_time();
        uint64_t t1 = mach_absolute_time();
        samples[13][i] = (double)(t1 - t0);
    }
}

/*
 * Pearson correlation coefficient between two sample arrays.
 */
static double pearson(const double *x, const double *y, int n) {
    double sx = 0, sy = 0, sxx = 0, syy = 0, sxy = 0;
    for (int i = 0; i < n; i++) {
        sx += x[i];
        sy += y[i];
        sxx += x[i] * x[i];
        syy += y[i] * y[i];
        sxy += x[i] * y[i];
    }
    double num = n * sxy - sx * sy;
    double den = sqrt((n * sxx - sx * sx) * (n * syy - sy * sy));
    if (den < 1e-12) return 0.0;
    return num / den;
}

typedef void (*collect_fn)(void);

static collect_fn collectors[N_SOURCES] = {
    collect_thread_lifecycle,
    collect_mach_ipc,
    collect_tlb_shootdown,
    collect_pipe_buffer,
    collect_kqueue_events,
    collect_dvfs_race,
    collect_cas_contention,
    collect_denormal_timing,
    collect_fsync_journal,
    collect_nvme_latency,
    collect_pdn_resonance,
    collect_amx_timing,
    collect_mach_timing,
    collect_clock_jitter,
};

int main(void) {
    printf("Full Correlation Audit — %d samples per source\n\n", N_SAMPLES);

    /* Collect from all sources. */
    for (int s = 0; s < N_SOURCES; s++) {
        printf("Collecting: %-20s ... ", SOURCE_NAMES[s]);
        fflush(stdout);
        collectors[s]();
        printf("done\n");
    }

    printf("\n=== CORRELATION MATRIX ===\n\n");

    /* Print header. */
    printf("%-20s", "");
    for (int j = 0; j < N_SOURCES; j++)
        printf(" %8.8s", SOURCE_NAMES[j]);
    printf("\n");

    int flagged = 0;

    for (int i = 0; i < N_SOURCES; i++) {
        printf("%-20s", SOURCE_NAMES[i]);
        for (int j = 0; j < N_SOURCES; j++) {
            double r = pearson(samples[i], samples[j], N_SAMPLES);
            printf(" %8.4f", r);
            if (i < j && fabs(r) > 0.15) {
                flagged++;
            }
        }
        printf("\n");
    }

    printf("\n=== FLAGGED PAIRS (|r| > 0.15) ===\n\n");
    for (int i = 0; i < N_SOURCES; i++) {
        for (int j = i + 1; j < N_SOURCES; j++) {
            double r = pearson(samples[i], samples[j], N_SAMPLES);
            if (fabs(r) > 0.15) {
                printf("  WARNING: %s <-> %s : r = %.4f\n",
                       SOURCE_NAMES[i], SOURCE_NAMES[j], r);
            }
        }
    }

    if (flagged == 0) {
        printf("  (none — all pairs have |r| <= 0.15)\n");
    }

    printf("\nTotal flagged pairs: %d\n", flagged);
    return 0;
}
