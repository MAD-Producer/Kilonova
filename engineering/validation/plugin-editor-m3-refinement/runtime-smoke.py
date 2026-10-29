import ctypes, json, os, subprocess, time
from pathlib import Path
from PIL import ImageGrab

ROOT = Path(os.environ.get('MOTION_PLUGIN_AUTHOR_RUNTIME_DIR',r'E:/Dev/aem/artifacts/plugin-editor-m3-refinement/runtime-native'))
EXE = Path(os.environ['TEMP'])/'motion-plugin-editor-m3-refinement/motion-studio.exe'
SOURCE = Path(r'E:/Dev/aem/.worktrees/plugin-editor/crates/motion-graph/examples/parameter-workbench.msplugin')
ROOT.mkdir(parents=True, exist_ok=True)
user32 = ctypes.windll.user32
user32.SetProcessDPIAware()
user32.GetWindowTextW.argtypes=[ctypes.c_void_p,ctypes.c_wchar_p,ctypes.c_int]
user32.PostMessageW.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_size_t,ctypes.c_ssize_t]
user32.SetWindowPos.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_uint]
user32.GetClientRect.argtypes=[ctypes.c_void_p,ctypes.c_void_p]
user32.ClientToScreen.argtypes=[ctypes.c_void_p,ctypes.c_void_p]
class RECT(ctypes.Structure):
    _fields_=[('left',ctypes.c_long),('top',ctypes.c_long),('right',ctypes.c_long),('bottom',ctypes.c_long)]
class POINT(ctypes.Structure):
    _fields_=[('x',ctypes.c_long),('y',ctypes.c_long)]
CALLBACK=ctypes.WINFUNCTYPE(ctypes.c_bool,ctypes.c_void_p,ctypes.c_void_p)

def owner(pid):
    found=[]
    @CALLBACK
    def visit(hwnd,_):
        process=ctypes.c_ulong()
        user32.GetWindowThreadProcessId(ctypes.c_void_p(hwnd),ctypes.byref(process))
        if process.value==pid and user32.IsWindowVisible(ctypes.c_void_p(hwnd)):
            title=ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd,title,256)
            rect=RECT(); user32.GetClientRect(hwnd,ctypes.byref(rect))
            if title.value.startswith('Motion Studio') and rect.right>0 and rect.bottom>0:
                found.append(hwnd)
        return True
    user32.EnumWindows(visit,None)
    return found[0] if found else None

def capture(hwnd,path):
    # Raise only the known child briefly; do not search by an ambiguous title.
    user32.SetWindowPos(hwnd,ctypes.c_void_p(-1),0,0,0,0,0x0013)
    time.sleep(.25)
    r=RECT(); p=POINT()
    user32.GetClientRect(hwnd,ctypes.byref(r));user32.ClientToScreen(hwnd,ctypes.byref(p))
    image=ImageGrab.grab(bbox=(p.x,p.y,p.x+r.right,p.y+r.bottom),all_screens=True)
    user32.SetWindowPos(hwnd,ctypes.c_void_p(-2),0,0,0,0,0x0013)
    image.save(path)
    assert max(image.getpixel((5,5))[:3])<120, 'white/occluded client'
    return image

def click(hwnd,x,y):
    user32.SetWindowPos(hwnd,ctypes.c_void_p(-1),0,0,0,0,0x0013)
    user32.SetForegroundWindow(ctypes.c_void_p(hwnd))
    origin=POINT();user32.ClientToScreen(hwnd,ctypes.byref(origin))
    user32.SetCursorPos(origin.x+x,origin.y+y)
    time.sleep(.1)
    coords=(y<<16)|x
    user32.PostMessageW(hwnd,0x200,0,coords)
    user32.PostMessageW(hwnd,0x201,1,coords)
    time.sleep(.08)
    user32.PostMessageW(hwnd,0x202,0,coords)
    time.sleep(.5)
    user32.SetWindowPos(hwnd,ctypes.c_void_p(-2),0,0,0,0,0x0013)

report=[]
for mode,path in [('normal',SOURCE),('missing',ROOT/'missing.msplugin')]:
    out=ROOT/mode;out.mkdir(exist_ok=True)
    env=os.environ.copy();env['LOCALAPPDATA']=str(out/'appdata');env['MOTION_LOG']='info';env['MOTION_WGSL_ANALYZER']=str(EXE.parent/'wgsl-analyzer.exe')
    with (out/'stdout.txt').open('wb') as stdout,(out/'stderr.txt').open('wb') as stderr:
        process=subprocess.Popen([str(EXE),'--plugin-editor',str(path),'--locale','zh'],cwd=EXE.parent,env=env,stdout=stdout,stderr=stderr,creationflags=subprocess.CREATE_NO_WINDOW)
        hwnd=None
        try:
            deadline=time.monotonic()+45
            while time.monotonic()<deadline:
                assert process.poll() is None, f'{mode} exited early: {process.returncode}'
                hwnd=owner(process.pid)
                log=out/'appdata/MotionStudio/logs/motion-studio.log'
                text=log.read_text(encoding='utf-8',errors='replace') if log.exists() else ''
                marker='plugin_preview_values' if mode=='normal' else 'plugin.author error request=0 phase=read'
                if hwnd and marker in text: break
                time.sleep(.1)
            else: raise RuntimeError(f'{mode}: no ready window/log within 45 seconds')
            time.sleep(.5)
            image=capture(hwnd,out/'interface-wide.png')
            if mode=='normal':
                click(hwnd,70,115);click(hwnd,70,115); capture(hwnd,out/'components.png')
                origin=POINT();user32.ClientToScreen(hwnd,ctypes.byref(origin))
                user32.SetCursorPos(origin.x+80,origin.y+365)
                user32.PostMessageW(hwnd,0x200,0,(365<<16)|80)
                time.sleep(.7);capture(hwnd,out/'component-hover.png')
                user32.SetForegroundWindow(ctypes.c_void_p(hwnd))
                click(hwnd,208,52)
                time.sleep(.6);capture(hwnd,out/'guide.png')
                click(hwnd,25,52);time.sleep(1);capture(hwnd,out/'logic.png')
                click(hwnd,145,52); image=capture(hwnd,out/'preview.png')
                a=image.getpixel((30,130)); b=image.getpixel((400,500))
                assert a!=b and max(a[:3])-min(a[:3])>30, 'GPU preview missing'
                # Actual window resize, not a cropped mock canvas.
                user32.SetWindowPos(hwnd,None,0,0,784,700,0x0016)
                time.sleep(1)
                click(hwnd,70,52);capture(hwnd,out/'interface-narrow.png')
                click(hwnd,25,52);capture(hwnd,out/'logic-parameters-narrow.png')
                click(hwnd,117,118);capture(hwnd,out/'logic-wgsl-narrow.png')
                user32.SetForegroundWindow(ctypes.c_void_p(hwnd))
                click(hwnd,208,52)
                time.sleep(.5);capture(hwnd,out/'guide-narrow.png')
            user32.PostMessageW(hwnd,0x10,0,0)
            code=process.wait(timeout=20)
            assert code==(0 if mode=='normal' else 1),(mode,code)
            report.append({'mode':mode,'exit_code':code,'size':image.size,'pid':process.pid})
        finally:
            if process.poll() is None:
                process.kill();process.wait()
    assert (out/'stdout.txt').read_bytes()==b''
    if mode=='missing':
        log=(out/'appdata/MotionStudio/logs/motion-studio.log').read_text(encoding='utf-8',errors='replace')
        stderr=(out/'stderr.txt').read_text(encoding='utf-8',errors='replace')
        assert 'plugin.author error request=0 phase=read' in log and 'plugin.author error request=0 phase=read' in stderr
(ROOT/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(report,ensure_ascii=False))
