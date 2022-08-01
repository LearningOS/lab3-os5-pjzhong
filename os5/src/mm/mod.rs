mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_tale;

pub use address::*;
pub use memory_set::remap_test;
pub use memory_set::MapPermission;
pub use memory_set::MemorySet;
pub use memory_set::KERNEL_SPACE;
pub use page_tale::get_mut;
pub use page_tale::translated_byte_buffer;
pub use page_tale::PageTable;
pub use page_tale::PageTableEntry;

pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.lock().activate();
}
