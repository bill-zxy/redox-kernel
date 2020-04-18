use core::convert::TryFrom;

use crate::memory::Frame;
use crate::paging::{ActivePageTable, Page, PhysicalAddress, VirtualAddress};
use crate::paging::entry::EntryFlags;

/// RSDP
#[derive(Copy, Clone, Debug)]
#[repr(packed)]
pub struct RSDP {
    signature: [u8; 8],
    checksum: u8,
    oemid: [u8; 6],
    revision: u8,
    rsdt_address: u32,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3]
}

impl RSDP {
    fn is_acpi_1_0(&self) -> bool {
        self.revision == 0
    }
    fn is_acpi_2_0(&self) -> bool {
        self.revision == 2
    }
    fn get_already_supplied_rsdps(area: &[u8]) -> Option<RSDP> {
        // the bootloader has already checked all the checksums for us, but we still need to
        // double-check.
        struct Iter<'a> {
            buf: &'a [u8],
        }
        impl<'a> Iterator for Iter<'a> {
            type Item = &'a [u8];

            fn next(&mut self) -> Option<Self::Item> {
                let length = <[u8; 4]>::try_from(&self.buf[..4]).ok()?;
                if (4 + length as usize) > self.buf.len() { return None }
                self.buf = self.buf[4 + length..];
                Ok(length)
            }
        }
        fn slice_to_rsdp(slice: &[u8]) -> Option<&RSDP> {
            let ptr = slice.as_ptr() as usize;

            if slice.len() >= mem::size_of::<RSDP>() && ptr & (!0x7) == ptr {
                let rsdp = unsafe { &*(slice.as_ptr() as *const RSDP) };
                // TODO: Validate
                Some(rsdp)
            } else { None }
        }

        // first, find an RDSP for ACPI 2.0
        if let Some(rdsp_2_0) = Iter { buf: area }.filter_map(slice_to_rsdp).filter(|rsdp| rsdp.is_acpi_2_0()) {
            return Some(rsdp_2_0);
        }

        // secondly, find an RDSP for ACPI 1.0
        if let Some(rdsp_1_0) = Iter { buf: area }.filter_map(slice_to_rsdp).filter(|rsdp| rsdp.is_acpi_1_0()) {
            return Some(rsdp_1_0);
        }
    }
    pub fn get_rsdp(active_table: &mut ActivePageTable, already_supplied_rsdps: Option<(u64, u64)>) -> Option<RSDP> {
        if let Some((base, size)) = already_supplied_rsdps {
            let area = core::slice::from_raw_parts(base as usize as *const u8, size as usize);
            Self::get_already_supplied_rsdps(area)
        } else {
            Self::get_rsdp_by_searching(active_table)
        }
    }
    /// Search for the RSDP
    pub fn get_rsdp_by_searching(active_table: &mut ActivePageTable) -> Option<RSDP> {
        let start_addr = 0xE_0000;
        let end_addr = 0xF_FFFF;

        // Map all of the ACPI RSDP space
        {
            let start_frame = Frame::containing_address(PhysicalAddress::new(start_addr));
            let end_frame = Frame::containing_address(PhysicalAddress::new(end_addr));
            for frame in Frame::range_inclusive(start_frame, end_frame) {
                let page = Page::containing_address(VirtualAddress::new(frame.start_address().get()));
                let result = active_table.map_to(page, frame, EntryFlags::PRESENT | EntryFlags::NO_EXECUTE);
                result.flush(active_table);
            }
        }

        RSDP::search(start_addr, end_addr)
    }

    fn search(start_addr: usize, end_addr: usize) -> Option<RSDP> {
        for i in 0 .. (end_addr + 1 - start_addr)/16 {
            let rsdp = unsafe { &*((start_addr + i * 16) as *const RSDP) };
            if &rsdp.signature == b"RSD PTR " {
                return Some(*rsdp);
            }
        }
        None
    }

    /// Get the RSDT or XSDT address
    pub fn sdt_address(&self) -> usize {
        if self.revision >= 2 {
            self.xsdt_address as usize
        } else {
            self.rsdt_address as usize
        }
    }
}
