"""Read process CPU and memory without injecting instrumentation into either server."""
import sys,json,os
def measure(pid):
 if sys.platform=="win32":
  import ctypes as c
  from ctypes import wintypes as w
  class Memory(c.Structure):
   _fields_=[("cb",w.DWORD),("PageFaultCount",w.DWORD),("PeakWorkingSetSize",c.c_size_t),("WorkingSetSize",c.c_size_t),("QuotaPeakPagedPoolUsage",c.c_size_t),("QuotaPagedPoolUsage",c.c_size_t),("QuotaPeakNonPagedPoolUsage",c.c_size_t),("QuotaNonPagedPoolUsage",c.c_size_t),("PagefileUsage",c.c_size_t),("PeakPagefileUsage",c.c_size_t),("PrivateUsage",c.c_size_t)]
  k=c.WinDLL("kernel32",use_last_error=True);k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
  k.GetProcessTimes.argtypes=[w.HANDLE]+[c.POINTER(c.c_ulonglong)]*4;k.CloseHandle.argtypes=[w.HANDLE]
  ps=c.WinDLL("psapi");ps.GetProcessMemoryInfo.argtypes=[w.HANDLE,c.POINTER(Memory),w.DWORD]
  handle=k.OpenProcess(0x410,False,pid)
  if not handle:raise OSError(c.get_last_error())
  try:
   created,exited,kernel,user=[c.c_ulonglong() for _ in range(4)]
   if not k.GetProcessTimes(handle,c.byref(created),c.byref(exited),c.byref(kernel),c.byref(user)):raise OSError(c.get_last_error())
   mem=Memory();mem.cb=c.sizeof(Memory)
   if not ps.GetProcessMemoryInfo(handle,c.byref(mem),mem.cb):raise OSError(c.get_last_error())
   return {"pid":pid,"cpuMs":(kernel.value+user.value)/10000,"rssMiB":mem.WorkingSetSize/1048576,"privateMiB":mem.PrivateUsage/1048576,"peakRssMiB":mem.PeakWorkingSetSize/1048576}
  finally:k.CloseHandle(handle)
 elif sys.platform.startswith("linux"):
  stat=open(f"/proc/{pid}/stat").read().rsplit(")",1)[1].split();memory=open(f"/proc/{pid}/statm").read().split()
  return {"pid":pid,"cpuMs":(int(stat[11])+int(stat[12]))*1000/os.sysconf("SC_CLK_TCK"),"rssMiB":int(memory[1])*os.sysconf("SC_PAGE_SIZE")/1048576}
 else:raise RuntimeError("CPU/memory sampling supports Windows and Linux.")
print(json.dumps([measure(int(p)) for p in sys.argv[1:]]))
