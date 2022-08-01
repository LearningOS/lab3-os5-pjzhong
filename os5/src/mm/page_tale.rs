use _core::mem::size_of;
use alloc::vec;
use alloc::vec::Vec;
use bitflags::*;

use super::{
    address::{PhysPageNum, StepByOne, VirtPageNum},
    frame_allocator::{frame_alloc, FrameTracker},
    VirtAddr,
};

bitflags! {
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct PageTableEntry {
    pub bits: usize,
}

impl PageTableEntry {
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        PageTableEntry {
            bits: ppn.0 << 10 | flags.bits as usize,
        }
    }

    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }

    pub fn flags(&self) -> Option<PTEFlags> {
        PTEFlags::from_bits(self.bits as u8)
    }

    pub fn ppn(&self) -> PhysPageNum {
        (self.bits >> 10 & ((1usize << 44) - 1)).into()
    }

    pub fn is_valid(&self) -> bool {
        self.flags()
            .map_or(false, |f| (f & PTEFlags::V) != PTEFlags::empty())
    }

    pub fn readable(&self) -> bool {
        self.flags()
            .map_or(false, |f| (f & PTEFlags::R) != PTEFlags::empty())
    }
    pub fn writable(&self) -> bool {
        self.flags()
            .map_or(false, |f| (f & PTEFlags::W) != PTEFlags::empty())
    }
    pub fn executable(&self) -> bool {
        self.flags()
            .map_or(false, |f| (f & PTEFlags::X) != PTEFlags::empty())
    }
}

pub struct PageTable {
    root_ppn: PhysPageNum,
    frames: Vec<FrameTracker>,
}

impl PageTable {
    pub fn new() -> Self {
        let frame = frame_alloc().unwrap();
        Self {
            root_ppn: frame.ppn,
            frames: vec![frame],
        }
    }

    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) {
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(!pte.is_valid(), "vpn {:?} is mapped before mapping", vpn);
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V)
    }

    pub fn unmap(&mut self, vpn: VirtPageNum) {
        let pte = self.find_pte(vpn).unwrap();
        assert!(pte.is_valid(), "vpn {:?} is invalid before unmapping", vpn);
        *pte = PageTableEntry::empty()
    }

    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, item) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[*item];
            if i == 2 {
                result = Some(pte);
                break;
            }

            if !pte.is_valid() {
                //TODO handle this
                //中间节点
                let frame = frame_alloc().unwrap();
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V);
                self.frames.push(frame);
            }

            ppn = pte.ppn();
        }
        result
    }

    fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, item) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[*item];
            if i == 2 {
                result = Some(pte);
                break;
            }

            if pte.is_valid() {
                ppn = pte.ppn();
            } else {
                return None;
            }
        }
        result
    }

    pub fn from_token(satp: usize) -> Self {
        Self {
            root_ppn: PhysPageNum::from(satp & (1usize << 44) - 1),
            frames: Vec::new(),
        }
    }

    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.find_pte(vpn).map(|pte| *pte)
    }

    pub fn token(&self) -> usize {
        8usize << 60 | self.root_ppn.0
    }
}

pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static [u8]> {
    let page_table = PageTable::from_token(token);
    let mut start = ptr as usize;
    let end = start + len;
    let mut v = Vec::new();
    while start < end {
        let start_va = VirtAddr::from(start);
        let mut vpn = start_va.floor();
        let ppn = page_table.translate(vpn).unwrap().ppn();
        vpn.step();
        let mut end_va: VirtAddr = vpn.into();
        end_va = end_va.min(VirtAddr::from(end));
        v.push(&ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()]);
        start = end_va.into();
    }
    v
}

/// 获取单个分页之内的对象
pub fn get_mut<T>(token: usize, ptr: *mut T) -> Option<&'static mut T> {
    let start = ptr as usize;
    let page_table = PageTable::from_token(token);
    let start_va = VirtAddr::from(start);
    let end_va = VirtAddr::from(start + size_of::<T>());
    let page_table_entry = page_table.translate(start_va.floor());
    if let Some(page_table_entry) = page_table_entry {
        let ppn = page_table_entry.ppn();
        let buffers = &ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()];
        unsafe { (buffers.as_ptr() as *mut T).as_mut() }
    } else {
        None
    }
}
