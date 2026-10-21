"""Windows process ownership for the bounded local client test.
No global PID kills, breakaway flags, service changes, or privilege changes.
The parent retains its process/job handles. Worker starts suspended, is assigned
before execution, and its descendants inherit the non-breakaway job.
"""
from __future__ import annotations
import ctypes as C
from ctypes import wintypes as W
import os, subprocess, time, uuid

class BASIC_LIMIT(C.Structure):
    _fields_=[('user',C.c_longlong),('job',C.c_longlong),('flags',W.DWORD),('min',C.c_size_t),('max',C.c_size_t),('active',W.DWORD),('affinity',C.c_size_t),('priority',W.DWORD),('scheduling',W.DWORD)]
class IO(C.Structure):
    _fields_=[(n,C.c_ulonglong) for n in ('readOps','writeOps','otherOps','readBytes','writeBytes','otherBytes')]
class EXTENDED(C.Structure):
    _fields_=[('basic',BASIC_LIMIT),('io',IO),('processMemory',C.c_size_t),('jobMemory',C.c_size_t),('peakProcess',C.c_size_t),('peakJob',C.c_size_t)]
class ACCOUNTING(C.Structure):
    _fields_=[('user',C.c_longlong),('kernel',C.c_longlong),('periodUser',C.c_longlong),('periodKernel',C.c_longlong),('faults',W.DWORD),('total',W.DWORD),('active',W.DWORD),('terminated',W.DWORD)]
class STARTUP(C.Structure):
    _fields_=[('cb',W.DWORD),('reserved',W.LPWSTR),('desktop',W.LPWSTR),('title',W.LPWSTR),('x',W.DWORD),('y',W.DWORD),('width',W.DWORD),('height',W.DWORD),('xchars',W.DWORD),('ychars',W.DWORD),('fill',W.DWORD),('flags',W.DWORD),('show',W.WORD),('reservedSize',W.WORD),('reservedPtr',C.c_void_p),('stdin',W.HANDLE),('stdout',W.HANDLE),('stderr',W.HANDLE)]
class PROCESS(C.Structure):
    _fields_=[('process',W.HANDLE),('thread',W.HANDLE),('pid',W.DWORD),('tid',W.DWORD)]

def kernel():
    if os.name!='nt':raise OSError('Windows process containment required')
    k=C.WinDLL('kernel32',use_last_error=True)
    signatures={
      'CreateJobObjectW':(W.HANDLE,[C.c_void_p,W.LPCWSTR]),
      'OpenJobObjectW':(W.HANDLE,[W.DWORD,W.BOOL,W.LPCWSTR]),
      'SetInformationJobObject':(W.BOOL,[W.HANDLE,C.c_int,C.c_void_p,W.DWORD]),
      'QueryInformationJobObject':(W.BOOL,[W.HANDLE,C.c_int,C.c_void_p,W.DWORD,C.c_void_p]),
      'AssignProcessToJobObject':(W.BOOL,[W.HANDLE,W.HANDLE]),
      'IsProcessInJob':(W.BOOL,[W.HANDLE,W.HANDLE,C.POINTER(W.BOOL)]),
      'TerminateJobObject':(W.BOOL,[W.HANDLE,W.UINT]),
      'CloseHandle':(W.BOOL,[W.HANDLE]),
      'GetCurrentProcess':(W.HANDLE,[]),
      'CreateProcessW':(W.BOOL,[W.LPCWSTR,W.LPWSTR,C.c_void_p,C.c_void_p,W.BOOL,W.DWORD,C.c_void_p,W.LPCWSTR,C.POINTER(STARTUP),C.POINTER(PROCESS)]),
      'ResumeThread':(W.DWORD,[W.HANDLE]),
      'WaitForSingleObject':(W.DWORD,[W.HANDLE,W.DWORD]),
      'GetExitCodeProcess':(W.BOOL,[W.HANDLE,C.POINTER(W.DWORD)]),
      'TerminateProcess':(W.BOOL,[W.HANDLE,W.UINT]),
      'OpenProcess':(W.HANDLE,[W.DWORD,W.BOOL,W.DWORD]),
      'QueryFullProcessImageNameW':(W.BOOL,[W.HANDLE,W.DWORD,W.LPWSTR,C.POINTER(W.DWORD)])}
    for name,(result,args) in signatures.items():f=getattr(k,name);f.restype=result;f.argtypes=args
    return k

def check(value):
    if not value:raise C.WinError(C.get_last_error())
    return value

class ProcessHandle:
    def __init__(self,k,handle,pid):self.k=k;self.handle=handle;self.pid=pid
    def poll(self):
        state=self.k.WaitForSingleObject(self.handle,0)
        if state==258:return None
        if state!=0:raise C.WinError(C.get_last_error())
        code=W.DWORD();check(self.k.GetExitCodeProcess(self.handle,C.byref(code)));return code.value
    def wait(self,seconds):
        state=self.k.WaitForSingleObject(self.handle,max(0,int(seconds*1000)))
        if state==258:return None
        if state!=0:raise C.WinError(C.get_last_error())
        return self.poll()
    def image(self):
        n=W.DWORD(32768);buf=C.create_unicode_buffer(n.value)
        check(self.k.QueryFullProcessImageNameW(self.handle,0,buf,C.byref(n)));return buf.value
    def terminate(self,code=124):
        if self.poll() is None:check(self.k.TerminateProcess(self.handle,code))
    def close(self):
        if self.handle:check(self.k.CloseHandle(self.handle));self.handle=None

def retain_process(pid):
    k=kernel();h=check(k.OpenProcess(0x100000|0x1000|0x100|0x1,False,pid))
    return ProcessHandle(k,h,pid)

def current_job_membership(name,process=None,assign=False):
    k=kernel();h=check(k.OpenJobObjectW(0x4|(0x1 if assign else 0),False,name))
    try:
        target=process.handle if process else k.GetCurrentProcess();member=W.BOOL()
        check(k.IsProcessInJob(target,h,C.byref(member)))
        if not member.value and assign:
            check(k.AssignProcessToJobObject(h,target));check(k.IsProcessInJob(target,h,C.byref(member)))
        return bool(member.value)
    finally:check(k.CloseHandle(h))

class OwnedJob:
    def __init__(self):
        self.k=kernel();self.name='Local\\Metallic-NativeClient-'+uuid.uuid4().hex;self.handle=None
        C.set_last_error(0);h=check(self.k.CreateJobObjectW(None,self.name));already=C.get_last_error()==183
        if already:self.k.CloseHandle(h);raise RuntimeError('Unexpected existing job; refusing reuse')
        self.handle=h
        try:
            info=EXTENDED();info.basic.flags=0x2000  # KILL_ON_JOB_CLOSE, NO breakaway
            check(self.k.SetInformationJobObject(h,9,C.byref(info),C.sizeof(info)))
        except BaseException:self.close();raise
    def spawn(self,command,cwd):
        # No inherited handles. Empty security attributes give the current user's ACL.
        startup=STARTUP();startup.cb=C.sizeof(startup);process=PROCESS()
        line=C.create_unicode_buffer(subprocess.list2cmdline([str(x) for x in command]))
        check(self.k.CreateProcessW(str(command[0]),line,None,None,False,0x4|0x08000000,None,str(cwd),C.byref(startup),C.byref(process)))
        retained=ProcessHandle(self.k,process.process,process.pid)
        try:
            check(self.k.AssignProcessToJobObject(self.handle,process.process))
            member=W.BOOL();check(self.k.IsProcessInJob(process.process,self.handle,C.byref(member)))
            if not member.value:raise RuntimeError('Worker containment not established')
            if self.k.ResumeThread(process.thread)==0xffffffff:raise C.WinError(C.get_last_error())
            return retained
        except BaseException:
            retained.terminate();retained.wait(2);retained.close();raise
        finally:check(self.k.CloseHandle(process.thread))
    def accounting(self):
        data=ACCOUNTING();check(self.k.QueryInformationJobObject(self.handle,1,C.byref(data),C.sizeof(data),None))
        return {'totalProcesses':data.total,'activeProcesses':data.active,'terminatedByLimit':data.terminated}
    def stop(self,code=124):check(self.k.TerminateJobObject(self.handle,code))
    def wait_empty(self,seconds=3):
        end=time.monotonic()+seconds
        while time.monotonic()<end:
            if self.accounting()['activeProcesses']==0:return True
            time.sleep(.025)
        return self.accounting()['activeProcesses']==0
    def close(self):
        if self.handle:check(self.k.CloseHandle(self.handle));self.handle=None
    def __enter__(self):return self
    def __exit__(self,*args):self.close()
