//! Windows Job Object (kill-on-close): every upstream child is assigned to a
//! job object so that when this process exits — including being killed — the
//! kernel terminates all upstream processes **and their descendants**
//! (the `cmd /C -> npx -> node` grandchild chain). Hand-written kernel32
//! declarations, zero external dependencies.

#[cfg(windows)]
mod job {
    use std::sync::OnceLock;
    type Handle = isize;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct IoCounters([u64; 6]);
    #[repr(C)]
    struct BasicLimit {
        per_process_time: i64,
        per_job_time: i64,
        limit_flags: u32,
        min_working_set: usize,
        max_working_set: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }
    #[repr(C)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    unsafe extern "system" {
        fn CreateJobObjectW(attrs: *mut core::ffi::c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: i32,
            info: *mut core::ffi::c_void,
            len: u32,
        ) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn AssignProcessToJobObject(job: Handle, proc: Handle) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    static JOB: OnceLock<Handle> = OnceLock::new();

    fn job_handle() -> Handle {
        *JOB.get_or_init(|| unsafe {
            let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            let mut info: ExtendedLimit = std::mem::zeroed();
            info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &mut info as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<ExtendedLimit>() as u32,
            );
            if ok == 0 {
                eprintln!("[r2r-gateway:job] SetInformationJobObject failed; child cleanup falls back to transport kill");
            }
            job
        })
    }

    /// Assign a freshly spawned child (by pid) to the kill-on-close job.
    /// Idempotent per process; the job handle is never closed — it is
    /// released when this process exits, which is the point.
    pub fn assign_pid(pid: u32) -> bool {
        let job = job_handle();
        if job == 0 {
            return false;
        }
        unsafe {
            let proc = OpenProcess(PROCESS_TERMINATE | PROCESS_SET_QUOTA, 0, pid);
            if proc == 0 {
                eprintln!("[r2r-gateway:job] OpenProcess({pid}) failed");
                return false;
            }
            let ok = AssignProcessToJobObject(job, proc);
            CloseHandle(proc);
            if ok == 0 {
                eprintln!("[r2r-gateway:job] AssignProcessToJobObject({pid}) failed");
            }
            ok != 0
        }
    }
}

/// Assign a spawned upstream child to the kill-on-close job (Windows only).
#[cfg(windows)]
pub fn assign_child(pid: u32) -> bool {
    job::assign_pid(pid)
}

#[cfg(not(windows))]
pub fn assign_child(_pid: u32) -> bool {
    // Non-Windows platforms rely on the transport's Drop kill plus process
    // groups; no job-object equivalent is needed for the P0 skeleton.
    false
}
