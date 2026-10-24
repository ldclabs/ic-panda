import os,subprocess,time,mmap,json
path='/mnt/probe/fixture.env'
expected=("DMSG_PROBE='synthetic-only'\n"+"#fixture\n"*150000).encode()
a=os.open(path,os.O_RDONLY);b=os.open(path,os.O_RDONLY)
assert os.fstat(a).st_size==len(expected)
t=time.monotonic();assert os.pread(a,100,0)==expected[:100];delay_a=time.monotonic()-t
assert delay_a>=4.5
assert os.pread(a,31,1200000)==expected[1200000:1200031]
# A new handle still waits, despite the first being approved and its bytes cached.
t=time.monotonic();assert os.pread(b,100,0)==expected[:100];delay_b=time.monotonic()-t
assert delay_b>=4.5
try:os.pread(a,1,0);raise AssertionError('expired first handle read')
except PermissionError:pass
try:os.open(path,os.O_WRONLY);raise AssertionError('write allowed')
except OSError:pass
other=subprocess.run(['runuser','-u','nobody','--','head','-c','1',path],capture_output=True)
assert other.returncode!=0 and not other.stdout
os.close(a);os.close(b)
print(json.dumps({'first_handle_delay':round(delay_a,3),'second_handle_delay':round(delay_b,3),'expired_handle_denied':True,'cross_uid_denied':True,'random_read':True,'size':len(expected)}))
