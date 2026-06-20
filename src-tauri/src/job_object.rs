use std::sync::OnceLock;

// SAFETY: HANDLE is just a pointer, safe to share/send between threads in our usage.
struct JobHandle(isize);
unsafe impl Send for JobHandle {}
unsafe impl Sync for JobHandle {}

/// Windows Job Object — auto-kills child processes when the parent exits,
/// even on crash or abrupt termination (no cleanup code needed).
/// Uses `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so closing the job handle
/// (which happens when this Rust process exits) terminates all children.
#[cfg(windows)]
pub(crate) fn register(child: &mut std::process::Child) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
        JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;

    static JOB: OnceLock<JobHandle> = OnceLock::new();

    let job = JOB.get_or_init(|| unsafe {
        let handle = CreateJobObjectW(None, PCWSTR::null()).unwrap_or_default();
        if !handle.is_invalid() {
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let _ = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
        }
        JobHandle(handle.0 as isize)
    });

    let handle = HANDLE(job.0 as *mut std::ffi::c_void);
    if !handle.is_invalid() {
        unsafe {
            let _ = AssignProcessToJobObject(handle, HANDLE(child.as_raw_handle()));
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn register(_child: &mut std::process::Child) {}
