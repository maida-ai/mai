//! Job assignment is part of CreateProcess, before any candidate code runs.
use anyhow::{Result, ensure};
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::SECURITY_ATTRIBUTES,
    System::{
        JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Pipes::CreatePipe,
        Threading::{
            CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

pub(super) struct Supervision(OwnedHandle);
impl Supervision {
    pub(super) fn quiescent(&self) -> Result<bool> {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        ensure!(
            unsafe {
                QueryInformationJobObject(
                    self.0.as_raw_handle(),
                    JobObjectBasicAccountingInformation,
                    (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                    std::ptr::null_mut(),
                )
            } != 0,
            "query validator job: {}",
            io::Error::last_os_error()
        );
        Ok(info.ActiveProcesses == 0)
    }
    pub(super) fn terminate(&mut self) -> Result<()> {
        ensure!(
            unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } != 0,
            "terminate validator job: {}",
            io::Error::last_os_error()
        );
        Ok(())
    }
}
impl Drop for Supervision {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}
pub(super) struct Child {
    process: OwnedHandle,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
}
impl Child {
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(ExitStatus::from_raw(code)))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
}
fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let result: Vec<_> = value.encode_wide().collect();
    ensure!(!result.contains(&0), "NUL in process argument/path");
    Ok(result)
}
fn quote(value: &OsStr) -> Result<Vec<u16>> {
    let mut output = vec![34];
    let mut slashes = 0;
    for unit in wide(value)? {
        if unit == 92 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            92,
            if unit == 34 { 2 * slashes + 1 } else { slashes },
        ));
        slashes = 0;
        output.push(unit);
    }
    output.extend(std::iter::repeat_n(92, 2 * slashes));
    output.push(34);
    Ok(output)
}
fn pipe() -> Result<(File, OwnedHandle)> {
    let mut read: HANDLE = std::ptr::null_mut();
    let mut write: HANDLE = std::ptr::null_mut();
    let security = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    ensure!(
        unsafe { CreatePipe(&mut read, &mut write, &security, 0) } != 0,
        "validator pipe: {}",
        io::Error::last_os_error()
    );
    let read = unsafe { File::from_raw_handle(read) };
    let write = unsafe { OwnedHandle::from_raw_handle(write) };
    ensure!(
        unsafe { SetHandleInformation(read.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } != 0,
        "validator pipe inheritance: {}",
        io::Error::last_os_error()
    );
    Ok((read, write))
}
struct Attributes(Vec<usize>);
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.0.as_mut_ptr().cast());
        }
    }
}

fn executable(program: &OsStr) -> Result<PathBuf> {
    let path = Path::new(program);
    if path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
    }) {
        anyhow::bail!(
            "batch-script programs are unsupported; configure an explicit native interpreter executable and declared script dependency"
        );
    }
    if path.components().count() > 1 || path.is_absolute() {
        return Ok(std::fs::canonicalize(path)?);
    }
    let mut names = vec![program.to_owned()];
    if path.extension().is_none() {
        for suffix in [".exe", ".com"] {
            let mut name = program.to_owned();
            name.push(suffix);
            names.push(name);
        }
    }
    // Resolve before switching to candidate cwd; CreateProcess's implicit
    // working-directory search must never select a candidate-supplied tool.
    if let Some(search) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&search) {
            for name in &names {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    return Ok(std::fs::canonicalize(candidate)?);
                }
            }
        }
    }
    anyhow::bail!("native executable {:?} not found on service PATH", program)
}
pub(super) fn spawn(command: &Command) -> Result<(Supervision, Child)> {
    let executable = executable(command.get_program())?;
    let mut program = wide(executable.as_os_str())?;
    program.push(0);
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    ensure!(
        !job.is_null(),
        "create validator Job Object: {}",
        io::Error::last_os_error()
    );
    let supervision = Supervision(unsafe { OwnedHandle::from_raw_handle(job) });
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    ensure!(
        unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } != 0,
        "configure validator job: {}",
        io::Error::last_os_error()
    );
    let (stdout, out_writer) = pipe()?;
    let (stderr, err_writer) = pipe()?;
    let null = OpenOptions::new().read(true).open("NUL")?;
    ensure!(
        unsafe {
            SetHandleInformation(
                null.as_raw_handle(),
                HANDLE_FLAG_INHERIT,
                HANDLE_FLAG_INHERIT,
            )
        } != 0,
        "inherit null stdin"
    );
    let mut handles = [
        null.as_raw_handle(),
        out_writer.as_raw_handle(),
        err_writer.as_raw_handle(),
    ];
    let mut jobs = [job];
    let mut bytes = 0;
    unsafe {
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut bytes);
    }
    ensure!(bytes > 0, "query validator attributes");
    let mut storage = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
    ensure!(
        unsafe { InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 2, 0, &mut bytes) }
            != 0,
        "initialize validator attributes: {}",
        io::Error::last_os_error()
    );
    let mut attributes = Attributes(storage);
    for (kind, pointer, size) in [
        (
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            handles.as_mut_ptr().cast(),
            std::mem::size_of_val(&handles),
        ),
        (
            PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
            jobs.as_mut_ptr().cast(),
            std::mem::size_of_val(&jobs),
        ),
    ] {
        ensure!(
            unsafe {
                UpdateProcThreadAttribute(
                    attributes.0.as_mut_ptr().cast(),
                    0,
                    kind,
                    pointer,
                    size,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            } != 0,
            "set validator process attributes (Windows 10+ Job List required): {}",
            io::Error::last_os_error()
        );
    }
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = null.as_raw_handle();
    startup.StartupInfo.hStdOutput = out_writer.as_raw_handle();
    startup.StartupInfo.hStdError = err_writer.as_raw_handle();
    startup.lpAttributeList = attributes.0.as_mut_ptr().cast();
    let mut arguments = quote(executable.as_os_str())?;
    for argument in command.get_args() {
        arguments.push(32);
        arguments.extend(quote(argument)?);
    }
    arguments.push(0);
    let mut environment: std::collections::BTreeMap<_, _> = std::env::vars_os().collect();
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            environment.insert(key.to_owned(), value.to_owned());
        } else {
            environment.remove(key);
        }
    }
    let mut block = Vec::new();
    for (key, value) in environment {
        block.extend(wide(&key)?);
        block.push(61);
        block.extend(wide(&value)?);
        block.push(0);
    }
    block.push(0);
    let mut directory = command
        .get_current_dir()
        .map(|path| wide(path.as_os_str()))
        .transpose()?;
    if let Some(directory) = &mut directory {
        directory.push(0);
    }
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    ensure!(
        unsafe {
            CreateProcessW(
                program.as_ptr(),
                arguments.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                block.as_ptr().cast(),
                directory
                    .as_ref()
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
                &startup.StartupInfo,
                &mut info,
            )
        } != 0,
        "spawn validator with atomic job assignment: {}",
        io::Error::last_os_error()
    );
    unsafe {
        CloseHandle(info.hThread);
    }
    Ok((
        supervision,
        Child {
            process: unsafe { OwnedHandle::from_raw_handle(info.hProcess) },
            stdout: Some(stdout),
            stderr: Some(stderr),
        },
    ))
}
