/* Bounded native replacement for the Python thread sampler. Diagnostic only. */
#include <dirent.h>
#include <errno.h>
#include <inttypes.h>
#include <libproc.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define THREADS 32
struct thread_sample { uint64_t tid; int bytes, error; uint64_t cpu; int state, current, base, maximum; };
struct sample {
    uint64_t stamp, end, observer_cpu;
    int task_bytes, task_error, count;
    struct proc_taskinfo task;
    struct thread_sample threads[THREADS];
};
static uint64_t clock_ns(clockid_t clock) {
    struct timespec value;
    if (clock_gettime(clock, &value)) { perror("clock_gettime"); exit(2); }
    return (uint64_t)value.tv_sec * 1000000000 + (uint64_t)value.tv_nsec;
}
static int identities(const char *directory, int pid, uint64_t *ids) {
    DIR *dir = opendir(directory);
    if (!dir) return -1;
    int count = 0;
    struct dirent *entry;
    while ((entry = readdir(dir))) {
        int owner; uint64_t tid; int used = 0;
        if (sscanf(entry->d_name, "%d-%" SCNu64 ".tid%n", &owner, &tid, &used) == 2
            && used && entry->d_name[used] == '\0' && owner == pid) {
            if (count == THREADS) { closedir(dir); return -2; }
            ids[count++] = tid;
        }
    }
    closedir(dir);
    return count;
}
int main(int argc, char **argv) {
    if (argc != 5) { fprintf(stderr, "usage: sampler PID SECONDS IDENTITIES OUTPUT\n"); return 2; }
    int pid = atoi(argv[1]), seconds = atoi(argv[2]);
    if (pid <= 0 || seconds < 1 || seconds > 600) return 2;
    size_t capacity = (size_t)seconds * 100 + 2, count = 0;
    struct sample *samples = calloc(capacity, sizeof(*samples));
    if (!samples) return 2;
    uint64_t start = clock_ns(CLOCK_MONOTONIC), until = start + (uint64_t)seconds * 1000000000;
    uint64_t next_ids = 0, ids[THREADS]; int nids = 0, overflow = 0;
    while (clock_ns(CLOCK_MONOTONIC) < until && count < capacity) {
        struct sample *s = &samples[count++];
        s->stamp = clock_ns(CLOCK_MONOTONIC);
        s->observer_cpu = clock_ns(CLOCK_THREAD_CPUTIME_ID);
        if (s->stamp >= next_ids) {
            nids = identities(argv[3], pid, ids);
            if (nids < 0) { overflow = nids; break; }
            next_ids = s->stamp + 1000000000;
        }
        errno = 0;
        s->task_bytes = proc_pidinfo(pid, PROC_PIDTASKINFO, 0, &s->task, sizeof(s->task));
        s->task_error = errno;
        if (s->task_bytes != sizeof(s->task)) break;
        s->count = nids;
        for (int i = 0; i < nids; i++) {
            struct thread_sample *t = &s->threads[i]; t->tid = ids[i]; errno = 0;
            struct proc_threadinfo info = {0};
            t->bytes = proc_pidinfo(pid, PROC_PIDTHREADID64INFO, t->tid, &info, sizeof(info));
            t->error = errno; t->cpu = info.pth_user_time + info.pth_system_time;
            t->state = info.pth_run_state; t->current = info.pth_curpri;
            t->base = info.pth_priority; t->maximum = info.pth_maxpriority;
        }
        s->end = clock_ns(CLOCK_MONOTONIC);
        struct timespec wait = { .tv_sec = 0, .tv_nsec = 10000000 };
        while (nanosleep(&wait, &wait) && errno == EINTR) {}
    }
    FILE *out = fopen(argv[4], "w");
    if (!out) { free(samples); return 2; }
    fprintf(out, "{\"pid\":%d,\"interval_seconds\":0.01,\"native_sampler\":true,\"identity_error\":%d,\"note\":\"sampled states are not continuous scheduler tracing; observer gaps are retained\"}\n", pid, overflow);
    for (size_t i = 0; i < count; i++) {
        struct sample *s = &samples[i];
        fprintf(out, "{\"monotonic_ns\":%" PRIu64 ",\"sample_end_ns\":%" PRIu64 ",\"observer_cpu_ns\":%" PRIu64 ",\"threads\":[", s->stamp, s->end, s->observer_cpu);
        for (int j = 0; j < s->count; j++) {
            struct thread_sample *t = &s->threads[j]; 
            if (j) fputc(',', out);
            if (t->bytes != sizeof(struct proc_threadinfo)) fprintf(out, "{\"tid\":%" PRIu64 ",\"error\":%d,\"returned_bytes\":%d}", t->tid, t->error, t->bytes);
            else fprintf(out, "{\"tid\":%" PRIu64 ",\"cpu_ns\":%" PRIu64 ",\"state\":%d,\"priority\":%d,\"base_priority\":%d,\"max_priority\":%d}", t->tid, t->cpu, t->state, t->current, t->base, t->maximum);
        }
        if (s->task_bytes == sizeof(s->task)) fprintf(out, "],\"task_counters\":{\"faults\":%d,\"pageins\":%d,\"cow_faults\":%d,\"csw\":%d,\"syscalls_mach\":%d,\"syscalls_unix\":%d,\"threadnum\":%d,\"numrunning\":%d,\"total_user_raw\":%" PRIu64 ",\"total_system_raw\":%" PRIu64 "}}\n", s->task.pti_faults, s->task.pti_pageins, s->task.pti_cow_faults, s->task.pti_csw, s->task.pti_syscalls_mach, s->task.pti_syscalls_unix, s->task.pti_threadnum, s->task.pti_numrunning, s->task.pti_total_user, s->task.pti_total_system);
        else fprintf(out, "],\"task_counters\":{\"error\":%d,\"returned_bytes\":%d}}\n", s->task_error, s->task_bytes);
    }
    int failed = ferror(out); if (fclose(out)) failed = 1;
    free(samples);
    return failed || overflow ? 2 : 0;
}
