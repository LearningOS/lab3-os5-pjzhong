use alloc::{collections::VecDeque, sync::Arc};
use lazy_static::lazy_static;

use crate::sync::UPSafeCell;

use super::task::TaskControlBlock;

pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }

    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        let res = self
            .ready_queue
            .binary_search_by_key(&task.inner_exclusive_access().pass, |task| {
                task.inner_exclusive_access().pass
            });

        match res {
            Ok(idx) | Err(idx) => {
                self.ready_queue.insert(idx, task);
            }
        }
    }

    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue.pop_front()
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}
