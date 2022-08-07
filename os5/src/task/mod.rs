mod context;
mod manager;
mod pid;
mod processor;
mod switch;
mod task;

use alloc::{sync::Arc, vec::Vec};
pub use context::TaskContext;
pub use manager::add_task;
pub use processor::{
    current_task, current_trap_cx, current_user_token, run_tasks, schedule, take_current_task,
};
pub use task::TaskStatus;

use self::task::TaskControlBlock;
use crate::{
    loader::get_app_data_by_name,
    mm::{get_mut, translated_refmut, MapPermission, VPNRange, VirtAddr},
    syscall::TaskInfo,
    timer::get_time_ms,
};
use lazy_static::*;

pub fn suspend_current_and_run_next() {
    //There must be an application running.
    let task = take_current_task().unwrap();

    // --- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);

    // push back to ready queue.
    add_task(task);
    // jump to scheduling cycle
    schedule(task_cx_ptr);
}

pub fn exit_current_and_run_next(exit_code: i32) {
    let curr_task = take_current_task().unwrap();
    let mut curr_task_inner = curr_task.inner_exclusive_access();
    curr_task_inner.task_status = TaskStatus::Zombie;
    curr_task_inner.exit_code = exit_code;

    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in curr_task_inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }

    curr_task_inner.children.clear();
    curr_task_inner.memory_set.recyle_data_page();
    drop(curr_task_inner);
    drop(curr_task);
    schedule(&mut TaskContext::zero_init() as *mut _)
}

pub fn reocrd_sys_call(sys_call_id: usize) {
    if let Some(task) = current_task() {
        task.inner_exclusive_access().syscall_times[sys_call_id] += 1;
    }
}

pub fn get_task_info(ti: *mut TaskInfo) {
    if let (Some(ti), Some(current)) = (translated_refmut(current_user_token(), ti), current_task())
    {
        let current = current.inner_exclusive_access();
        ti.status = current.task_status;
        ti.time = get_time_ms() - current.time;
        ti.syscall_times = current.syscall_times;
    }
}

struct TaskManagerInner {
    tasks: Vec<TaskControlBlock>,
    current_task: usize,
}

// YOUR JOB: 扩展内核以实现 sys_mmap 和 sys_munmap
pub fn do_sys_mmap(start: usize, len: usize, port: usize) -> isize {
    if port & !0x7 != 0 || port & 0x7 == 0 {
        return -1;
    }

    let start_va = VirtAddr::from(start);
    if !start_va.aligned() {
        return -1;
    }
    let end_va = VirtAddr::from(start + len).ceil();

    let current_task = current_task().unwrap();
    let memory_set = &mut current_task.inner_exclusive_access().memory_set;

    for i in VPNRange::new(start_va.into(), end_va) {
        if let Some(pte) = memory_set.translate(i) {
            if pte.is_valid() {
                return -1;
            }
        }
    }

    let mut map_perm = MapPermission::U;
    if port & 1 == 1 {
        map_perm |= MapPermission::R;
    }
    if port & 2 == 2 {
        map_perm |= MapPermission::W;
    }
    if port & 3 == 3 {
        map_perm |= MapPermission::X;
    }

    memory_set.insert_framed_area(start_va, end_va.into(), map_perm);

    0
}

pub fn do_sys_munmap(start: usize, len: usize) -> isize {
    let start_va = VirtAddr::from(start);
    if !start_va.aligned() {
        return -1;
    }
    let current_task = current_task().unwrap();
    let memory_set = &mut current_task.inner_exclusive_access().memory_set;

    let end_va = VirtAddr::from(start + len).ceil();

    for i in VPNRange::new(start_va.into(), end_va) {
        if let Some(pte) = memory_set.translate(i) {
            if pte.is_valid() {
                continue;
            }
        }
        return -1;
    }

    for i in VPNRange::new(start_va.into(), end_va) {
        memory_set.unmap(i);
    }

    0
}

lazy_static! {
    pub static ref INITPROC: Arc<TaskControlBlock> = Arc::new(TaskControlBlock::new(
        get_app_data_by_name("ch5b_initproc").unwrap()
    ));
}

pub fn add_initproc() {
    add_task(INITPROC.clone())
}
