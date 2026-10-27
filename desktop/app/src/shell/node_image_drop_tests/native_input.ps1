param([string]$Mode, [string]$File, [int]$X, [int]$Y, [long]$Handle)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -ReferencedAssemblies System.Windows.Forms,System.Drawing @"
using System;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;
public static class NativeNodeInput {
 static object traceLock = new object();
 public static void Mark(string path, string phase) {lock(traceLock) {System.IO.File.AppendAllText(path+".driver.log",DateTime.UtcNow.ToString("O")+" "+phase+Environment.NewLine);}}
 [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
 [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
 [DllImport("user32.dll")] static extern bool GetPhysicalCursorPos(out NativePoint point);
 [DllImport("user32.dll")] static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
 [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr window);
 [DllImport("user32.dll",SetLastError=true)] static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
 static void Expose(long handle) { if(!SetWindowPos(new IntPtr(handle),new IntPtr(-1),0,0,0,0,0x13)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); }
 [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
 [StructLayout(LayoutKind.Sequential)] struct NativePoint {public int x,y;}
 [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(NativePoint point);
 [StructLayout(LayoutKind.Sequential)] struct Rect { public int left,top,right,bottom; }
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rect);
 [DllImport("ole32.dll")] static extern int OleInitialize(IntPtr reserved);
 [DllImport("ole32.dll")] static extern void OleUninitialize();
 [DllImport("shell32.dll", CharSet=CharSet.Unicode)] static extern IntPtr ILCreateFromPathW(string path);
 [DllImport("shell32.dll")] static extern IntPtr ILFindLastID(IntPtr pidl);
 [DllImport("shell32.dll")] static extern void ILFree(IntPtr pidl);
 [DllImport("shell32.dll")] static extern int SHCreateDataObject(IntPtr folder, uint count, [MarshalAs(UnmanagedType.LPArray,SizeConst=1)] IntPtr[] children, IntPtr inner, ref Guid iid, out IntPtr data);
 [DllImport("shell32.dll")] static extern int SHDoDragDrop(IntPtr window, IntPtr data, IntPtr source, uint allowed, out uint effect);
 public static void Drop(string path, int x, int y, long handle) {
  SetThreadDpiAwarenessContext(new IntPtr(-4));
  path=System.IO.Path.GetFullPath(path);
  Mark(path,"initialize native shell source");
  int initialized = OleInitialize(IntPtr.Zero);
  if (initialized < 0) Marshal.ThrowExceptionForHR(initialized);
  IntPtr folder=IntPtr.Zero, file=IntPtr.Zero, data=IntPtr.Zero;
  try {
   folder=ILCreateFromPathW(System.IO.Path.GetDirectoryName(path));
   file=ILCreateFromPathW(path);
   if (folder==IntPtr.Zero || file==IntPtr.Zero) throw new Exception("native file PIDL missing: folder="+folder+" file="+file+" exists="+System.IO.File.Exists(path));
   Guid iid=new Guid("0000010e-0000-0000-C000-000000000046");
   Marshal.ThrowExceptionForHR(SHCreateDataObject(folder,1,new IntPtr[]{ILFindLastID(file)},IntPtr.Zero,ref iid,out data));
   using (var source = new Form()) {
    source.FormBorderStyle=FormBorderStyle.None; source.StartPosition=FormStartPosition.Manual;
    source.Location=new System.Drawing.Point(2,2); source.ClientSize=new System.Drawing.Size(32,32);
    source.TopMost=true; source.Show(); source.Update();
    var origin=source.PointToScreen(new System.Drawing.Point(16,16));
    if (!SetCursorPos(origin.X,origin.Y)) throw new Exception("source SetCursorPos failed");
    mouse_event(2,0,0,0,UIntPtr.Zero); Application.DoEvents();
    Mark(path,"source origin="+origin.X+","+origin.Y+" expected="+source.Handle+" actual="+WindowFromPoint(new NativePoint{x=origin.X,y=origin.Y})+" left="+GetAsyncKeyState(1));
    var probe=(System.Runtime.InteropServices.ComTypes.IDataObject)Marshal.GetObjectForIUnknown(data);
    var format=new System.Runtime.InteropServices.ComTypes.FORMATETC{cfFormat=15,dwAspect=System.Runtime.InteropServices.ComTypes.DVASPECT.DVASPECT_CONTENT,lindex=-1,tymed=System.Runtime.InteropServices.ComTypes.TYMED.TYMED_HGLOBAL};
    int formatResult=probe.QueryGetData(ref format);
    Mark(path,"CF_HDROP query="+formatResult);
    Marshal.ReleaseComObject(probe);
    Marshal.ThrowExceptionForHR(formatResult);
    if (GetAsyncKeyState(1)>=0) throw new Exception("native source did not receive a held left mouse button");
    var release=new Thread(() => {
     SetThreadDpiAwarenessContext(new IntPtr(-4));
     Thread.Sleep(200); Expose(handle); SetCursorPos(x,y); NativePoint physical; GetPhysicalCursorPos(out physical); Mark(path,"physical destination="+physical.x+","+physical.y); Mark(path,"at destination expected="+handle+" actual="+WindowFromPoint(new NativePoint{x=x,y=y})+" x="+x+" y="+y);
     Thread.Sleep(500); mouse_event(4,0,0,0,UIntPtr.Zero); Mark(path,"button released");
    });
    release.Start(); uint effect;
    Mark(path,"SHDoDragDrop enter");
    int result=SHDoDragDrop(source.Handle,data,IntPtr.Zero,1,out effect);
    Mark(path,"SHDoDragDrop exit "+result+" effect="+effect);
    release.Join(); source.Close();
    if (result!=0x40100 || effect!=1) throw new Exception("native shell drop failed: HRESULT="+result+" effect="+effect);
   }
  } finally {
   mouse_event(4,0,0,0,UIntPtr.Zero);
   if(data!=IntPtr.Zero)Marshal.Release(data); if(file!=IntPtr.Zero)ILFree(file); if(folder!=IntPtr.Zero)ILFree(folder);
   OleUninitialize();
  }
 }
 public static void Keys(string keys, long handle, int x, int y) {
  SetThreadDpiAwarenessContext(new IntPtr(-4));
  Rect rect;
  if (!GetWindowRect(new IntPtr(handle),out rect)) throw new Exception("GetWindowRect failed");
  Expose(handle);
  SetCursorPos(x==0 ? rect.left+100 : x,y==0 ? rect.top+10 : y);
  // Pace native gestures so Windows can dispatch the cursor move before the click.
  Thread.Sleep(50);
  mouse_event(2,0,0,0,UIntPtr.Zero); mouse_event(4,0,0,0,UIntPtr.Zero);
  Thread.Sleep(50);
  SetForegroundWindow(new IntPtr(handle));
  if (GetForegroundWindow() != new IntPtr(handle)) throw new Exception("Keyboard focus not granted: wanted=" + handle + " actual=" + GetForegroundWindow());
  SendKeys.SendWait(keys);
 }
}
"@
if ($Mode -eq 'drop') { [NativeNodeInput]::Drop($File,$X,$Y,$Handle) }
else { [NativeNodeInput]::Keys($File,$Handle,$X,$Y) }
