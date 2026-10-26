#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>
#include <time.h>
int main(int argc, char **argv) {
 if(argc!=2)return 2;uint64_t tid;pthread_threadid_np(NULL,&tid);char path[4096];
 snprintf(path,sizeof(path),"%s/%d-%llu.tid",argv[1],getpid(),(unsigned long long)tid);
 FILE *f=fopen(path,"w");if(!f)return 2;fclose(f);
 struct timespec begin, now;clock_gettime(CLOCK_MONOTONIC,&begin);
 do {clock_gettime(CLOCK_MONOTONIC,&now);} while((now.tv_sec-begin.tv_sec)+(now.tv_nsec-begin.tv_nsec)/1e9 < 1.0);return 0;
}
