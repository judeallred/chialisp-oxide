"""Transport controls add no policy/native/VM/SDK observations."""
import os
import subprocess
import sys
import unittest
import bounded_process as b


class ProcessTransport(unittest.TestCase):
    def command(self,source,**kwargs):
        return b.run([sys.executable,'-c',source],timeout=kwargs.pop('timeout',.5),stdout_limit=4096,stderr_limit=4096,**kwargs)

    def test_exact_two_stream_limits(self):
        r=self.command('import os;os.write(1,b"o"*4096);os.write(2,b"e"*4096)',check=True)
        self.assertEqual(r.stdout,b'o'*4096);self.assertEqual(r.stderr,b'e'*4096)

    def test_stdout_limit_before_accumulation(self):
        with self.assertRaises(b.OutputLimit) as caught:self.command('import os;os.write(1,b"x"*8192)')
        self.assertLessEqual(len(caught.exception.stdout),4096)

    def test_stderr_limit_before_accumulation(self):
        with self.assertRaises(b.OutputLimit) as caught:self.command('import os;os.write(2,b"x"*8192)')
        self.assertLessEqual(len(caught.exception.stderr),4096)

    def test_closed_streams_do_not_remove_deadline(self):
        with self.assertRaises(subprocess.TimeoutExpired):self.command('import os,time;os.close(1);os.close(2);time.sleep(30)')

    def test_timeout_reaps_parent_and_kills_descendant_group(self):
        source='import os,time;p=os.fork();print(os.getpid(),flush=True);time.sleep(30)'
        with self.assertRaises(subprocess.TimeoutExpired) as caught:self.command(source,timeout=2)
        self.assertEqual(len(caught.exception.stdout.decode().split()),2)
        with self.assertRaises(ChildProcessError):os.waitpid(caught.exception.pid,os.WNOHANG)
        for pid in caught.exception.stdout.decode().split():
            result=b.run(['ps','-o','stat=','-p',pid],timeout=2,stdout_limit=1024,stderr_limit=1024)
            self.assertTrue(not result.stdout.strip() or result.stdout.lstrip().startswith(b'Z'))

    def test_nonzero_exit_is_transport_result(self):
        with self.assertRaises(subprocess.CalledProcessError) as caught:self.command('import sys;print("bounded");sys.exit(3)',check=True)
        self.assertEqual(caught.exception.returncode,3);self.assertEqual(caught.exception.stdout,b'bounded\n')

if __name__=='__main__':unittest.main()
