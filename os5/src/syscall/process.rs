use crate::config::MAX_SYSCALL_NUM;
use crate::loader::get_app_data_by_name;
use crate::mm::get_mut;
use crate::mm::translated_str;
use crate::task::add_task;
use crate::task::current_task;
use crate::task::current_user_token;
use crate::task::do_sys_mmap;
use crate::task::do_sys_munmap;
use crate::task::exit_current_and_run_next;
use crate::task::get_task_info;
use crate::task::suspend_current_and_run_next;
use crate::task::TaskStatus;
use crate::timer::get_time_us;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[derive(Debug)]

pub struct TaskInfo {
    pub status: TaskStatus,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    pub time: usize,
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_exit(exit_code: i32) -> ! {
    println!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _: usize) -> isize {
    // let buffers =
    //     translated_byte_buffer(current_user_token(), ts as *const u8, size_of::<TimeVal>());

    // let start = ts as usize;
    // let page_table = PageTable::from_token(current_user_token());
    // let start_va = VirtAddr::from(start);
    // let end_va = VirtAddr::from(start + size_of::<TimeVal>());
    // let vpn = start_va.floor();
    // let ppn = page_table.translate(vpn).unwrap().ppn();
    // let buffers = &ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()];

    // let ts = unsafe { (buffers.as_ptr() as *mut TimeVal).as_mut() };
    if let Some(ts) = get_mut(current_user_token(), ts) {
        let us = get_time_us();
        ts.sec = us / 1_000_000;
        ts.usec = us % 1_000_000;
        0
    } else {
        -1
    }
}

/// YOUR JOB: Finish sys_task_info to pass testcases
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    get_task_info(ti);
    0
}

// YOUR JOB: 扩展内核以实现 sys_mmap 和 sys_munmap
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    do_sys_mmap(start, len, port)
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    do_sys_munmap(start, len)
}

pub fn sys_fork() -> isize {
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context fo new_task, because it return immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork return 0
    trap_cx.x[10] = 0; // x[10] is a0 reg
                       // add new task to sceduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let (Some(task), Some(data)) = (current_task(), get_app_data_by_name(path.as_str())) {
        task.exec(data);
        0
    } else {
        -1
    }
}
