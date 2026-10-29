"""POSIX child transport: precharged stream buffers, deadline and group cleanup.

Adapted from scripts/check-registry-codegen.py's selector transport. Transport
failures are inconclusive; they never establish a policy or consensus rejection.
"""
import os
import math
import selectors
import signal
import subprocess
import time


class OutputLimit(RuntimeError):
    def __init__(self,stream,stdout,stderr,pid):
        super().__init__(f'child {stream} byte limit exceeded')
        self.stdout=stdout;self.output=stdout;self.stderr=stderr;self.pid=pid


def run(argv,*,env=None,cwd=None,timeout=180,stdout_limit=16*1024*1024,stderr_limit=2*1024*1024,check=False):
    if os.name!='posix' or not isinstance(timeout,(int,float)) or not math.isfinite(timeout) or timeout<=0 or type(stdout_limit) is not int or type(stderr_limit) is not int or stdout_limit<0 or stderr_limit<0:
        raise ValueError('positive deadline and nonnegative stream bounds on POSIX required')
    argv=[str(x) for x in argv];buffers=[bytearray(),bytearray()];limits=[stdout_limit,stderr_limit]
    deadline=time.monotonic()+timeout
    child=subprocess.Popen(argv,env=env,cwd=cwd,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,start_new_session=True)
    selector=selectors.DefaultSelector()
    try:
        for index,stream in enumerate((child.stdout,child.stderr)):
            selector.register(stream,selectors.EVENT_READ,index)
        while selector.get_map():
            remaining_time=deadline-time.monotonic()
            if remaining_time<=0:
                error=subprocess.TimeoutExpired(argv,timeout,output=bytes(buffers[0]),stderr=bytes(buffers[1]));error.pid=child.pid;raise error
            for key,_ in selector.select(min(.25,remaining_time)):
                index=key.data;remaining=limits[index]-len(buffers[index])
                # At most one over-budget byte is read into scratch space; it
                # is rejected before extending either retained output buffer.
                chunk=os.read(key.fd,min(65536,remaining+1))
                if not chunk:
                    selector.unregister(key.fileobj);continue
                if len(chunk)>remaining:
                    buffers[index].extend(chunk[:remaining])
                    raise OutputLimit(('stdout','stderr')[index],bytes(buffers[0]),bytes(buffers[1]),child.pid)
                buffers[index].extend(chunk)
        remaining_time=deadline-time.monotonic()
        if remaining_time<=0:
            error=subprocess.TimeoutExpired(argv,timeout,output=bytes(buffers[0]),stderr=bytes(buffers[1]));error.pid=child.pid;raise error
        try:code=child.wait(timeout=remaining_time)
        except subprocess.TimeoutExpired as original:
            error=subprocess.TimeoutExpired(argv,timeout,output=bytes(buffers[0]),stderr=bytes(buffers[1]));error.pid=child.pid;raise error from original
    except BaseException:
        try:os.killpg(child.pid,signal.SIGKILL)
        except ProcessLookupError:pass
        child.wait(timeout=5)
        raise
    finally:
        selector.close();child.stdout.close();child.stderr.close()
    result=subprocess.CompletedProcess(argv,code,bytes(buffers[0]),bytes(buffers[1]))
    if check and code:
        raise subprocess.CalledProcessError(code,argv,output=result.stdout,stderr=result.stderr)
    return result
