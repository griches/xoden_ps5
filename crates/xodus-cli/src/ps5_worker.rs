//! Opt-in console worker diagnostics. No browser, child processes or Mac service.
use std::{fs::OpenOptions, io, os::windows::io::IntoRawHandle};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};

#[derive(Default)]
struct View { phase: String, completed: u64, total: Option<u64>, rate: u64 }
static VIEW: OnceLock<Mutex<View>> = OnceLock::new();

pub fn phase(value: &str) {
    if let Some(view) = VIEW.get() { if let Ok(mut view) = view.lock() { view.phase = value.into(); } }
}
pub fn progress(record: &serde_json::Value) {
    if let Some(view) = VIEW.get() { if let Ok(mut view) = view.lock() {
        view.completed = record["completedBytes"].as_u64().unwrap_or(0);
        view.total = record["totalBytes"].as_u64();
        view.rate = record["bytesPerSecond"].as_u64().unwrap_or(0);
    } }
}
fn wide(value: &str) -> Vec<u16> { value.encode_utf16().chain(Some(0)).collect() }

unsafe extern "system" fn window_proc(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // Win32 owns the callback and calls it only on the window's message thread.
    unsafe {
        match message {
            WM_TIMER => { InvalidateRect(window, std::ptr::null(), 0); 0 }
            WM_CLOSE => 0,
            WM_DESTROY => { PostQuitMessage(0); 0 }
            WM_PAINT => {
                let mut paint: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(window, &mut paint);
                let mut rect: RECT = std::mem::zeroed(); GetClientRect(window, &mut rect);
                // The console loader suppresses frames near black until it sees
                // content. This visible background also works before fonts load.
                let brush = CreateSolidBrush(0x242b20); FillRect(dc, &rect, brush); DeleteObject(brush);
                SetBkMode(dc, TRANSPARENT as i32); SetTextColor(dc, 0xeeeeee);
                let font = CreateFontW(42,0,0,0,500,0,0,0,DEFAULT_CHARSET as u32,0,0,0,0,wide("Segoe UI").as_ptr());
                let old = SelectObject(dc, font);
                let (label, numbers) = if let Some(view) = VIEW.get().and_then(|v|v.lock().ok()) {
                    let label = match view.phase.as_str() {
                        "checking-account" => "Checking your Xbox subscription",
                        "refreshing" => "Loading your Xbox library",
                        "resolving" => "Finding the PC game package",
                        "downloading" => "Downloading directly to your PS5",
                        "preparing" => "Preparing the game",
                        "prepared" => "Download prepared",
                        "complete" => "Library updated",
                        "package-choice" => "This game needs a package selection in Playden",
                        "failed" => "The operation failed. Details are saved for Playden",
                        _ => "Starting Xbox services",
                    };
                    let numbers = if view.phase == "downloading" {
                        view.total.filter(|n|*n>0).map(|total| format!("{:.1}%   |   {:.1} / {:.1} MB   |   {:.1} MB/s",
                            (view.completed.min(total) as f64 / total as f64)*100.0,view.completed as f64/1e6,total as f64/1e6,view.rate as f64/1e6)).unwrap_or_else(||"Reading download size".into())
                    } else { String::new() };
                    (label, numbers)
                } else { ("Starting Xbox services", String::new()) };
                let text = wide(&format!("Xbox on Playden\n\n{label}\n\n{numbers}"));
                rect.left += 80; rect.right -= 80; rect.top = (rect.bottom / 2 - 140).max(40);
                let drawn = DrawTextW(dc,text.as_ptr(),-1,&mut rect,DT_CENTER | DT_WORDBREAK | DT_NOPREFIX);
                static FIRST_PAINT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
                if FIRST_PAINT.swap(false, std::sync::atomic::Ordering::Relaxed) { eprintln!("PS5_WORKER_UI paint width={} height={} text_height={drawn}", rect.right, rect.bottom); }
                SelectObject(dc, old); DeleteObject(font); EndPaint(window, &paint); GdiFlush(); 0
            }
            _ => DefWindowProcW(window,message,w,l),
        }
    }
}

pub fn start_window() -> io::Result<()> {
    VIEW.get_or_init(|| Mutex::new(View::default()));
    let (tx,rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let class = wide("XodusPlaydenWorker");
        let instance = GetModuleHandleW(std::ptr::null());
        let mut wc: WNDCLASSW = std::mem::zeroed(); wc.lpfnWndProc = Some(window_proc); wc.hInstance = instance; wc.lpszClassName = class.as_ptr();
        if RegisterClassW(&wc) == 0 { let _ = tx.send(false); return; }
        let window = CreateWindowExW(0,class.as_ptr(),wide("Xbox on Playden").as_ptr(),WS_POPUP | WS_VISIBLE,
            0,0,GetSystemMetrics(SM_CXSCREEN).max(1280),GetSystemMetrics(SM_CYSCREEN).max(720),std::ptr::null_mut(),std::ptr::null_mut(),instance,std::ptr::null());
        if window.is_null() { let _ = tx.send(false); return; }
        ShowWindow(window,SW_SHOW); SetForegroundWindow(window); UpdateWindow(window); SetTimer(window,1,250,None);
        let _ = tx.send(true);
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message,std::ptr::null_mut(),0,0) > 0 { TranslateMessage(&message); DispatchMessageW(&message); }
    });
    if rx.recv_timeout(std::time::Duration::from_secs(15)).unwrap_or(false) { Ok(()) }
    else { Err(io::Error::other("worker window unavailable")) }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetStdHandle(which: u32, handle: *mut core::ffi::c_void) -> i32;
}

pub fn initialize() -> io::Result<()> {
    // The parent directory is provisioned privately as the Playden user.
    for (which, name) in [(-11i32, "worker-progress.log"), (-12, "worker-errors.log")] {
        let file = OpenOptions::new().create(true).truncate(true).write(true)
            .open(format!(r"Z:\data\playden\xbox\{name}"))?;
        let handle = file.into_raw_handle();
        // The process owns the handle until exit; Rust stdout/stderr use it.
        if unsafe { SetStdHandle(which as u32, handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
