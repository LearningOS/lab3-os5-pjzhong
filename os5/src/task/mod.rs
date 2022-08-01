mod context;
mod pid;
mod switch;
mod task;

use alloc::vec::Vec;
pub use context::TaskContext;
pub use task::TaskStatus;

use self::{switch::__switch, task::TaskControlBlock};
use crate::{
    loader::{get_app_data, get_num_app},
    mm::{get_mut, MapPermission, StepByOne, VPNRange, VirtAddr},
    sync::UPSafeCell,
    syscall::TaskInfo,
    timer::get_time_ms,
    trap::TrapContext,
};
use lazy_static::*;

pub struct TaskManager {
    num_app: usize,
    inner: UPSafeCell<TaskManagerInner>,
}

fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

pub fn reocrd_sys_call(sys_call_id: usize) {
    let mut manager = TASK_MANAGER.inner.exclusive_access();
    let current = manager.current_task;
    manager.tasks[current].syscall_times[sys_call_id] += 1;
}

pub fn get_task_info(ti: *mut TaskInfo) {
    // let buffers =
    //     translated_byte_buffer(current_user_token(), ti as *const u8, size_of::<TaskInfo>());

    // let ti = unsafe { (buffers[0].as_ptr() as *mut TaskInfo).as_mut() };

    if let Some(ti) = get_mut(current_user_token(), ti) {
        let mamger = TASK_MANAGER.inner.exclusive_access();
        let current = &mamger.tasks[mamger.current_task];
        ti.status = current.task_status;
        ti.time = get_time_ms() - current.time;
        ti.syscall_times = current.syscall_times.clone()
    }
}

impl TaskManager {
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Ready;
    }

    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Exited;
    }

    fn run_next_task(&self) {
        if let Some(next) = self.find_next_taks() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            if inner.tasks[next].time == 0 {
                inner.tasks[next].time = get_time_ms();
            }
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            panic!("All applications completed!")
        }
    }

    fn find_next_taks(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let task0 = &mut inner.tasks[0];
        // task0.time = get_time_ms();
        task0.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &task0.task_cx as *const TaskContext;
        drop(inner);
        let mut unused = TaskContext::zero_init();
        unsafe { __switch(&mut unused as *mut _, next_task_cx_ptr) }
        panic!("unreachable in run_frist_task!")
    }

    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    fn get_current_trap_cx(&self) -> &mut TrapContext {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].get_trap_cx()
    }
}

pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

struct TaskManagerInner {
    tasks: Vec<TaskControlBlock>,
    current_task: usize,
}

lazy_static! {
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = get_num_app();
        println!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i))
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
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

    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current_task = inner.current_task;
    let memory_set = &mut inner.tasks[current_task].memory_set;

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
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current_task = inner.current_task;
    let memory_set = &mut inner.tasks[current_task].memory_set;

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
