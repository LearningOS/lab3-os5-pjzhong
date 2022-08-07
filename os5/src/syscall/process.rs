use alloc::sync::Arc;

use crate::config::MAX_SYSCALL_NUM;
use crate::loader::get_app_data_by_name;
use crate::mm::get_mut;
use crate::mm::translated_refmut;
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
    if let Some(ts) = translated_refmut(current_user_token(), ts) {
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

pub fn sys_spawn(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let (Some(task), Some(data)) = (current_task(), get_app_data_by_name(path.as_str())) {
        let new_task = task.spawn(data);
        add_task(new_task);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let curr_task = current_task().unwrap();
    let mut curr_task_inner = curr_task.inner_exclusive_access();

    if curr_task_inner
        .children
        .iter()
        .find(|p| pid == -1 || pid as usize == p.get_pid())
        .is_none()
    {
        return -1;
    }

    let pair = curr_task_inner.children.iter().enumerate().find(|(_, p)| {
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.get_pid())
    });

    if let Some((idx, _)) = pair {
        let child = curr_task_inner.children.remove(idx);
        // confirm that child will de deallocated after removing from child list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.get_pid();

        let exit_code = child.inner_exclusive_access().exit_code;
        if let Some(ptr) = translated_refmut(curr_task_inner.memory_set.token(), exit_code_ptr) {
            *ptr = exit_code;
        }
        found_pid as isize
    } else {
        -2
    }
}
