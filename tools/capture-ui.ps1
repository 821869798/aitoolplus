param(
  [string]$Name = "screen",
  [Nullable[int]]$ClickX = $null,
  [Nullable[int]]$ClickY = $null,
  [Nullable[int]]$HoverX = $null,
  [Nullable[int]]$HoverY = $null,
  [Nullable[int]]$ScrollX = $null,
  [Nullable[int]]$ScrollY = $null,
  [int]$ScrollDelta = 0
)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class NativeUi {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr dpiContext);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  public struct RECT { public int Left, Top, Right, Bottom; }
  public struct POINT { public int X, Y; }
  public static void ClickClientFromScreen(IntPtr h, int screenX, int screenY) {
    POINT p = new POINT { X = screenX, Y = screenY };
    ScreenToClient(h, ref p);
    int packed = (p.Y << 16) | (p.X & 0xffff);
    PostMessage(h, 0x0200, UIntPtr.Zero, new IntPtr(packed));
    PostMessage(h, 0x0201, new UIntPtr(1), new IntPtr(packed));
    PostMessage(h, 0x0202, UIntPtr.Zero, new IntPtr(packed));
  }
  public static void MouseWheel(IntPtr h, int screenX, int screenY, int delta) {
    mouse_event(0x0800, 0, 0, unchecked((uint)delta), UIntPtr.Zero);
    int wparam = delta << 16;
    int lparam = (screenY << 16) | (screenX & 0xffff);
    PostMessage(h, 0x020A, new UIntPtr(unchecked((uint)wparam)), new IntPtr(lparam));
  }
  public static IntPtr FindMainWindow(uint targetPid) {
    IntPtr best = IntPtr.Zero;
    int bestArea = 0;
    EnumWindows((h, l) => {
      uint pid;
      GetWindowThreadProcessId(h, out pid);
      if (pid == targetPid && IsWindowVisible(h)) {
        RECT r;
        GetWindowRect(h, out r);
        int area = (r.Right - r.Left) * (r.Bottom - r.Top);
        if (area > bestArea) {
          bestArea = area;
          best = h;
        }
      }
      return true;
    }, IntPtr.Zero);
    return best;
  }
}
'@
try { [NativeUi]::SetProcessDpiAwarenessContext([IntPtr](-4)) } catch {}
$proc = Get-Process aitoolplus -ErrorAction Stop | Select-Object -First 1
$hwnd = [NativeUi]::FindMainWindow($proc.Id)
if ($hwnd -eq [IntPtr]::Zero) { $hwnd = $proc.MainWindowHandle }
if ($hwnd -eq [IntPtr]::Zero) { throw "aitoolplus window handle is zero" }
[NativeUi]::ShowWindow($hwnd, 9) | Out-Null
[NativeUi]::BringWindowToTop($hwnd) | Out-Null
[NativeUi]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 900
$rect = New-Object NativeUi+RECT
[NativeUi]::ShowWindow($hwnd, 9) | Out-Null
[NativeUi]::SetForegroundWindow($hwnd) | Out-Null
[NativeUi]::BringWindowToTop($hwnd) | Out-Null
[NativeUi]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
if ($ClickX -ne $null -and $ClickY -ne $null) {
  $sx = $rect.Left + $ClickX
  $sy = $rect.Top + $ClickY
  [NativeUi]::SetCursorPos($sx, $sy) | Out-Null
  Start-Sleep -Milliseconds 250
  [NativeUi]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 120
  [NativeUi]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  # Also post a client-coordinate click; this is reliable with GPUI/DirectComposition.
  [NativeUi]::ClickClientFromScreen($hwnd, $sx, $sy)
  Start-Sleep -Milliseconds 1200
  [NativeUi]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
}
if ($ScrollDelta -ne 0 -and $ScrollX -ne $null -and $ScrollY -ne $null) {
  $sx = $rect.Left + $ScrollX
  $sy = $rect.Top + $ScrollY
  [NativeUi]::SetCursorPos($sx, $sy) | Out-Null
  Start-Sleep -Milliseconds 250
  [NativeUi]::MouseWheel($hwnd, $sx, $sy, $ScrollDelta)
  Start-Sleep -Milliseconds 600
  [NativeUi]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
}
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$printed = [NativeUi]::PrintWindow($hwnd, $hdc, 2)
$g.ReleaseHdc($hdc)
$out = "D:\program\rust\aitoolplus\docs\screenshots\$Name.png"
[System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($out)) | Out-Null
$bmp.Save($out)
$g.Dispose(); $bmp.Dispose()
Write-Host "$Name $w x $h at $($rect.Left),$($rect.Top)"
