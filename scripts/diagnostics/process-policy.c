#include <stdio.h>
#include <unistd.h>
#include <pthread.h>
#include <pthread/qos.h>
#include <sys/resource.h>
#include <libproc.h>
#include <mach/mach.h>
#include <mach/task_policy.h>
static void *read_policy(void *arg) {
 uint64_t tid=0; pthread_threadid_np(NULL,&tid);
 struct proc_threadinfo t={0}; int n=proc_pidinfo(getpid(),PROC_PIDTHREADID64INFO,tid,&t,sizeof(t));
 qos_class_t qos=QOS_CLASS_UNSPECIFIED; int rel=0; int q=pthread_get_qos_class_np(pthread_self(),&qos,&rel);
 task_category_policy_data_t cat={0}; mach_msg_type_number_t count=TASK_CATEGORY_POLICY_COUNT; boolean_t def=FALSE;
 int c=task_policy_get(mach_task_self(),TASK_CATEGORY_POLICY,(task_policy_t)&cat,&count,&def);
 printf("{\"thread\":\"%s\",\"pid\":%d,\"nice\":%d,\"qos_status\":%d,\"qos\":%u,\"relative\":%d,\"task_role_status\":%d,\"task_role\":%d,\"threadinfo_bytes\":%d,\"current_priority\":%d,\"base_priority\":%d,\"max_priority\":%d}\n",(char*)arg,getpid(),getpriority(PRIO_PROCESS,0),q,qos,rel,c,cat.role,n,t.pth_curpri,t.pth_priority,t.pth_maxpriority);
 return NULL;
}
int main(void) {read_policy("main");pthread_t t;int e=pthread_create(&t,NULL,read_policy,"child");if(e)return e;return pthread_join(t,NULL);}
