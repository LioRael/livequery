// Diagnostic control: no LiveQuery code, locks, allocation or I/O in measured work.
#include <time.h>
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>
static uint64_t ns(clockid_t id) { struct timespec t; if(clock_gettime(id,&t)) _exit(2); return (uint64_t)t.tv_sec*1000000000ULL+t.tv_nsec; }
struct sample {uint64_t start,wall,cpu;};
static struct sample samples[200];
int main(void) {
 volatile uint64_t value=1;
 struct timespec pause={0,50000000};
 for(int i=0;i<200;i++) {
  uint64_t wall=ns(CLOCK_MONOTONIC),cpu=ns(CLOCK_THREAD_CPUTIME_ID),end;
  do {for(int j=0;j<1024;j++) value=value*1664525+1013904223;end=ns(CLOCK_THREAD_CPUTIME_ID);}while(end-cpu<1000000);
  samples[i]=(struct sample){wall,ns(CLOCK_MONOTONIC)-wall,end-cpu};
  nanosleep(&pause,0);
 }
 for(int i=0;i<200;i++) printf("{\"start_ns\":%llu,\"wall_ns\":%llu,\"cpu_ns\":%llu}\n",(unsigned long long)samples[i].start,(unsigned long long)samples[i].wall,(unsigned long long)samples[i].cpu);
 return value==0;
}
