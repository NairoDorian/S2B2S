//! Windows job object that ties child processes to this app's lifetime.
//!
//! `llama-server` is a long-lived child. If the app dies without running its
//! exit hook (crash, task-manager kill), a plain `Command::spawn` child would
//! keep the GPU and the port. Assigning it to a job created with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` makes the kernel end it when the last
//! handle to the job — held by this process — goes away. One job for the
//! whole app; `register` is a no-op on other platforms.

#[cfg(windows)]
pub fn register(child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    struct Job(HANDLE);
    // SAFETY: the job handle is only ever passed to Win32 calls from any
    // thread; it is never closed while the process lives.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    static JOB: OnceLock<Option<Job>> = OnceLock::new();

    let job = JOB.get_or_init(|| unsafe {
        let handle = match CreateJobObjectW(None, None) {
            Ok(h) => h,
            Err(e) => {
                log::warn!(
                    "CreateJobObjectW failed: {e}; child processes will not be tied to {}",
                    crate::app_identity::NAME
                );
                return None;
            }
        };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Err(e) = SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) {
            log::warn!("SetInformationJobObject failed: {e}");
            return None;
        }
        Some(Job(handle))
    });

    if let Some(job) = job {
        let process = HANDLE(child.as_raw_handle() as *mut core::ffi::c_void);
        if let Err(e) = unsafe { AssignProcessToJobObject(job.0, process) } {
            log::warn!("AssignProcessToJobObject failed: {e}");
        }
    }
}

#[cfg(not(windows))]
pub fn register(_child: &std::process::Child) {}
