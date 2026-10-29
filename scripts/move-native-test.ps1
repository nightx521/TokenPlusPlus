param([int]$TargetPid,[int]$X,[int]$Y)
$ErrorActionPreference='Stop'
if ((Get-Process -Id $TargetPid).ProcessName -ne 'token-plusplus') { throw 'Only the test application may be moved' }
Add-Type -TypeDefinition @"
using System;using System.Runtime.InteropServices;
public static class TokenNativeMove {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,System.Text.StringBuilder s,int n);
 [StructLayout(LayoutKind.Sequential)] struct RECT{public int L,T,R,B;}
 [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h,out RECT r);
 public delegate bool Callback(IntPtr h,IntPtr d);
 [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb,IntPtr d);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll",EntryPoint="GetWindowLongPtrW")] static extern IntPtr Style(IntPtr h,int i);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int height,uint flags);
 [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 public static bool Move(int pid,int x,int y){bool moved=false;var old=SetThreadDpiAwarenessContext(new IntPtr(-4));try{EnumWindows((h,d)=>{uint p;GetWindowThreadProcessId(h,out p);RECT rect;GetClientRect(h,out rect);if(p==pid && rect.R-rect.L>200 && rect.B-rect.T>100 && IsWindowVisible(h) && (Style(h,-16).ToInt64() & 0x40000000)==0){var name=new System.Text.StringBuilder(128);GetClassName(h,name,128);Console.WriteLine("moved_class="+name.ToString()+" hwnd="+h.ToInt64());moved=SetWindowPos(h,IntPtr.Zero,x,y,0,0,0x0015);return false;}return true;},IntPtr.Zero);}finally{SetThreadDpiAwarenessContext(old);}return moved;}
}
"@
if (![TokenNativeMove]::Move($TargetPid,$X,$Y)) { throw 'Test window could not be moved' }
