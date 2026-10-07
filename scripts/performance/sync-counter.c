/* Benchmark-only instrumentation. Never bypasses or delays fdatasync. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

static uint64_t *counts;
static int (*real_sync)(int);
__attribute__((constructor)) static void init_counter(void) {
    real_sync = dlsym(RTLD_NEXT, "fdatasync");
    const char *path = getenv("OPERON_SYNC_COUNTER");
    if (!real_sync || !path) _exit(125);
    int fd = open(path, O_RDWR|O_CREAT|O_CLOEXEC|O_NOFOLLOW, 0600);
    if (fd < 0 || ftruncate(fd, 3*sizeof(uint64_t)) != 0) _exit(125);
    counts = mmap(NULL, 3*sizeof(uint64_t), PROT_READ|PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (counts == MAP_FAILED) _exit(125);
}
int fdatasync(int fd) {
    struct timespec start, end;
    clock_gettime(CLOCK_MONOTONIC, &start);
    int result = real_sync(fd);
    int saved_errno = errno;
    clock_gettime(CLOCK_MONOTONIC, &end);
    uint64_t elapsed = (end.tv_sec-start.tv_sec)*1000000000LL + end.tv_nsec-start.tv_nsec;
    __atomic_fetch_add(&counts[0], 1, __ATOMIC_RELAXED);
    __atomic_fetch_add(&counts[1], elapsed, __ATOMIC_RELAXED);
    if (result) __atomic_fetch_add(&counts[2], 1, __ATOMIC_RELAXED);
    errno = saved_errno;
    return result;
}
