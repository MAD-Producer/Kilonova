import ctypes, json, os, subprocess, time
from pathlib import Path
from PIL import ImageGrab

ROOT = Path(os.environ.get('MOTION_PLUGIN_AUTHOR_RUNTIME_DIR',r'E:/Dev/aem/artifacts/plugin-editor-host-entry/runtime-boxed'))
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


user32.GetClassNameW.argtypes=[ctypes.c_void_p,ctypes.c_wchar_p,ctypes.c_int]
user32.IsWindowVisible.argtypes=[ctypes.c_void_p]
user32.SendMessageW.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_size_t,ctypes.c_void_p]
def windows(pid,dialog=False):
    found=[]
    @CALLBACK
    def visit(hwnd,_):
        p=ctypes.c_ulong();user32.GetWindowThreadProcessId(ctypes.c_void_p(hwnd),ctypes.byref(p))
        if p.value==pid and user32.IsWindowVisible(ctypes.c_void_p(hwnd)):
            title=ctypes.create_unicode_buffer(256);user32.GetWindowTextW(hwnd,title,256)
            klass=ctypes.create_unicode_buffer(256);user32.GetClassNameW(hwnd,klass,256)
            if (dialog and klass.value=='#32770') or (not dialog and title.value.startswith('Motion Studio')):found.append((hwnd,title.value))
        return True
    user32.EnumWindows(visit,None);return found
def wait(predicate,label):
    start=time.monotonic()
    while time.monotonic()-start<45:
        assert process.poll() is None, f'process ended before {label}'
        value=predicate()
        if value:return value
        time.sleep(.05)
    raise RuntimeError('timeout: '+label)
env=os.environ.copy();env['LOCALAPPDATA']=str(ROOT/'appdata');env['MOTION_LOG']='info';env['MOTION_WGSL_ANALYZER']=str(EXE.parent/'wgsl-analyzer.exe')
with (ROOT/'stdout.txt').open('wb') as stdout,(ROOT/'stderr.txt').open('wb') as stderr:
    process=subprocess.Popen([str(EXE),'--project',str(ROOT/'project'),'--locale','zh'],cwd=EXE.parent,env=env,stdout=stdout,stderr=stderr,creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        main=wait(lambda:owner(process.pid),'main window');time.sleep(2)
        click(main,1100,250) # activate owned client before the menu click
        click(main,350,34);capture(main,ROOT/'window-menu.png')
        click(main,350,64)
        dialog=wait(lambda:windows(process.pid,True),'native source picker')[0][0]
        edits=[]
        @CALLBACK
        def visit(hwnd,_):
            klass=ctypes.create_unicode_buffer(256);user32.GetClassNameW(hwnd,klass,256)
            if klass.value=='Edit' and user32.IsWindowVisible(ctypes.c_void_p(hwnd)):edits.append(hwnd)
            return True
        user32.EnumChildWindows(ctypes.c_void_p(dialog),visit,None)
        assert edits,'owned file picker has no filename edit'
        text=ctypes.create_unicode_buffer(str(SOURCE.resolve()))
        user32.SendMessageW(edits[-1],0x0c,0,ctypes.cast(text,ctypes.c_void_p))
        user32.PostMessageW(dialog,0x111,1,0)
        child=wait(lambda:next((h for h,t in windows(process.pid) if h!=main and '插件编辑器' in t),None),'same-process author window')
        log=ROOT/'appdata/MotionStudio/logs/motion-studio.log'
        wait(lambda:log.exists() and 'plugin_preview_values' in log.read_text(encoding='utf-8',errors='replace'),'author GPU install')
        time.sleep(1);capture(child,ROOT/'hosted-editor.png')
        click(child,208,52);capture(child,ROOT/'hosted-guide.png')
        user32.PostMessageW(child,0x10,0,0)
        wait(lambda:len(windows(process.pid))==1,'child retirement')
        assert process.poll() is None
        capture(main,ROOT/'main-after-tool-close.png')
        user32.PostMessageW(main,0x10,0,0)
        assert process.wait(timeout=25)==0
    finally:
        if process.poll() is None:
            for h,t in windows(process.pid,True):user32.PostMessageW(h,0x111,2,0)
            for h,t in windows(process.pid):user32.PostMessageW(h,0x10,0,0)
            try:process.wait(timeout=10)
            except subprocess.TimeoutExpired:process.kill();process.wait()
assert (ROOT/'stdout.txt').read_bytes()==b''
(ROOT/'report.json').write_text(json.dumps({'pid':process.pid,'same_process_windows':True,'menu_source_picker':True,'tool_close_preserves_main':True,'exit_code':process.returncode,'stdout_empty':True},ensure_ascii=False,indent=2),encoding='utf-8')
print('native shell menu and same-process editor passed')
