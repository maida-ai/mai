//! Spawn a background Windows owner with an explicit inherited-handle allowlist.
use super::*;
use std::{
    ffi::OsStr,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::Threading::{
        CREATE_NO_WINDOW, CreateProcessW, DeleteProcThreadAttributeList,
        EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, GetExitCodeProcess,
        InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
        STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
    },
};

pub(super) struct Child(OwnedHandle);
impl Child {
    pub(super) fn try_wait(&mut self) -> io::Result<Option<u32>> {
        // SAFETY: process handle is owned and valid; zero timeout never waits.
        match unsafe { WaitForSingleObject(self.0.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                // SAFETY: valid process and writable exit-code pointer.
                if unsafe { GetExitCodeProcess(self.0.as_raw_handle(), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(code))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
}
fn inheritable(file: &File) -> Result<OwnedHandle> {
    let mut handle: HANDLE = std::ptr::null_mut();
    // SAFETY: valid source/current-process handles, writable destination;
    // only this new duplicate is inheritable, never the caller's handle.
    ensure!(
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                file.as_raw_handle(),
                GetCurrentProcess(),
                &mut handle,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            )
        } != 0,
        "duplicate background stdio: {}",
        io::Error::last_os_error()
    );
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
struct Attributes(Vec<usize>);
impl Attributes {
    fn new(handles: &mut [HANDLE]) -> Result<Self> {
        let mut size = 0;
        // SAFETY: the first call asks Windows for the required allocation.
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
        }
        ensure!(
            size > 0,
            "query process attributes: {}",
            io::Error::last_os_error()
        );
        // usize backing guarantees pointer alignment for the opaque list.
        let mut storage = vec![0; size.div_ceil(std::mem::size_of::<usize>())];
        ensure!(
            unsafe {
                InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 1, 0, &mut size)
            } != 0,
            "initialize process attributes: {}",
            io::Error::last_os_error()
        );
        let mut attributes = Self(storage);
        // SAFETY: initialized list and stable handle array remain alive
        // until after CreateProcessW and deletion of this attribute list.
        ensure!(
            unsafe {
                UpdateProcThreadAttribute(
                    attributes.0.as_mut_ptr().cast(),
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    handles.as_mut_ptr().cast(),
                    std::mem::size_of_val(handles),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            } != 0,
            "set background handle allowlist: {}",
            io::Error::last_os_error()
        );
        Ok(attributes)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: this owns one successfully initialized attribute list.
        unsafe {
            DeleteProcThreadAttributeList(self.0.as_mut_ptr().cast());
        }
    }
}
fn quoted(arg: &OsStr) -> Result<Vec<u16>> {
    let mut out = vec![b'"' as u16];
    let mut slashes = 0;
    for unit in arg.encode_wide() {
        ensure!(unit != 0, "NUL in background command argument");
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        out.push(unit);
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    out.push(b'"' as u16);
    Ok(out)
}
pub(super) fn spawn_background(root: &Path, log: File) -> Result<Child> {
    let exe = std::env::current_exe()?;
    let mut command = Vec::new();
    for arg in [
        exe.as_os_str(),
        OsStr::new("serve"),
        root.as_os_str(),
        OsStr::new("--foreground"),
    ] {
        if !command.is_empty() {
            command.push(b' ' as u16);
        }
        command.extend(quoted(arg)?);
    }
    command.push(0);
    let program: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let null = OpenOptions::new().read(true).write(true).open("NUL")?;
    let null = inheritable(&null)?;
    let log = inheritable(&log)?;
    let mut handles = [null.as_raw_handle(), log.as_raw_handle()];
    let mut attributes = Attributes::new(&mut handles)?;
    // SAFETY: zero initializes these POD Win32 structures; relevant fields follow.
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = null.as_raw_handle();
    startup.StartupInfo.hStdOutput = null.as_raw_handle();
    startup.StartupInfo.hStdError = log.as_raw_handle();
    startup.lpAttributeList = attributes.0.as_mut_ptr().cast();
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: executable/command are NUL-terminated and all referenced
    // buffers/handles/attributes live through this call. The explicit
    // allowlist prevents inheritance of the status caller's output pipes.
    ensure!(
        unsafe {
            CreateProcessW(
                program.as_ptr(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                std::ptr::null(),
                std::ptr::null(),
                &startup.StartupInfo,
                &mut info,
            )
        } != 0,
        "spawn background engine: {}",
        io::Error::last_os_error()
    );
    // SAFETY: successful CreateProcessW returns two owned handles. The
    // thread handle is unnecessary; process ownership moves to Child.
    unsafe {
        CloseHandle(info.hThread);
    }
    Ok(Child(unsafe {
        OwnedHandle::from_raw_handle(info.hProcess)
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_command_arguments_quote_spaces_quotes_and_trailing_slashes() {
        for (input, expected) in [
            ("", "\"\""),
            (r"C:\space path\", "\"C:\\space path\\\\\""),
            ("a\"b", "\"a\\\"b\""),
        ] {
            assert_eq!(
                String::from_utf16(&quoted(OsStr::new(input)).unwrap()).unwrap(),
                expected
            );
        }
        assert!(quoted(OsStr::new("bad\0argument")).is_err());
    }
}
