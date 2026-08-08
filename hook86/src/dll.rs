use std::ffi::c_void;
use std::io::{Error as IoError, ErrorKind};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use thiserror::Error;
use windows::core::{PCSTR, Error as WinError};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError,
    WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    HANDLE,
};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::System::Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE};
use windows::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeThread, OpenProcess, WaitForSingleObject,
    PROCESS_ACCESS_RIGHTS, PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};

pub use hook86_dll_main::dll_main;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CallReason {
    ProcessAttach { is_static_load: bool },
    ProcessDetach { is_process_exiting: bool },
    ThreadAttach,
    ThreadDetach,
}

const KERNEL32: PCSTR = PCSTR::from_raw(b"kernel32.dll\0".as_ptr());
const LOAD_LIBRARY_W: PCSTR = PCSTR::from_raw(b"LoadLibraryW\0".as_ptr());
const INJECT_ACCESS: PROCESS_ACCESS_RIGHTS = PROCESS_ACCESS_RIGHTS(PROCESS_CREATE_THREAD.0 | PROCESS_QUERY_INFORMATION.0 | PROCESS_VM_OPERATION.0 | PROCESS_VM_READ.0 | PROCESS_VM_WRITE.0);
const INJECT_TIMEOUT: u32 = 10000;

#[derive(Debug, Error)]
pub enum InjectError {
    #[error(transparent)]
    OsError(#[from] WinError),
    #[error(transparent)]
    IoError(#[from] IoError),
    #[error("DLL load timed out")]
    LoadTimeout,
    #[error("DLL load failed")]
    LoadFailed,
    #[error("Unknown error: {0}")]
    UnknownError(String),
}

fn win_error() -> Result<(), InjectError> {
    unsafe { Err(InjectError::OsError(GetLastError().to_hresult().into())) }
}

struct ResourceContainer {
    process: HANDLE,
    buf: *mut c_void,
    thread: HANDLE,
}

impl ResourceContainer {
    fn new(process: HANDLE) -> Self {
        Self { process, buf: std::ptr::null_mut(), thread: HANDLE::default() }
    }
}

impl Drop for ResourceContainer {
    fn drop(&mut self) {
        unsafe {
            if !self.thread.is_invalid() {
                let _ = CloseHandle(self.thread);
            }
            if !self.buf.is_null() {
                let _ = VirtualFreeEx(self.process, self.buf, 0, MEM_RELEASE);
            }
            let _ = CloseHandle(self.process);
        }
    }
}

/// Inject the provided DLL into the specified process.
///
/// Note that the target process must have the same architecture as the current process.
pub fn inject(dll_path: impl AsRef<Path>, pid: u32) -> Result<(), InjectError> {
    // need an absolute path because our working directory may not be the same as the target process
    let dll_path = std::fs::canonicalize(dll_path.as_ref())?;
    if !dll_path.is_file() {
        return Err(InjectError::IoError(IoError::new(ErrorKind::IsADirectory, format!("Path {} is not a DLL file", dll_path.display()))));
    }

    let wide_path: Vec<u16> = dll_path.as_os_str()
        .encode_wide()
        .chain(Some(0)) // null terminator
        .collect();
    let path_size = wide_path.len() * 2;

    unsafe {
        let kernel32 = GetModuleHandleA(KERNEL32)?;
        let load_library_w = match GetProcAddress(kernel32, LOAD_LIBRARY_W) {
            None => return win_error(),
            ptr => std::mem::transmute(ptr),
        };

        let mut res = ResourceContainer::new(OpenProcess(INJECT_ACCESS, false, pid)?);

        res.buf = VirtualAllocEx(res.process, None, path_size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if res.buf.is_null() {
            return win_error();
        }

        WriteProcessMemory(res.process, res.buf, wide_path.as_ptr() as *const c_void, path_size, None)?;

        res.thread = CreateRemoteThread(res.process, None, 0, load_library_w, Some(res.buf), 0, None)?;

        match WaitForSingleObject(res.thread, INJECT_TIMEOUT) {
            WAIT_OBJECT_0 => (),
            WAIT_TIMEOUT => {
                // don't free the buffer because the remote thread may still be using it
                res.buf = std::ptr::null_mut();
                return Err(InjectError::LoadTimeout);
            }
            WAIT_FAILED => {
                res.buf = std::ptr::null_mut();
                return win_error();
            }
            // can't get WAIT_ABANDONED because we're not waiting on a mutex
            result => return Err(InjectError::UnknownError(format!("Unexpected WaitForSingleObject result: {}", result.0))),
        }

        let mut exit_code = 0u32;
        GetExitCodeThread(res.thread, &mut exit_code)?;

        // exit code will be the return value of LoadLibraryW, which is NULL on failure
        match exit_code {
            0 => Err(InjectError::LoadFailed),
            _ => Ok(()),
        }
    }
}